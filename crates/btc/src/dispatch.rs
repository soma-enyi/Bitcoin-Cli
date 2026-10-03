//! Routes each parsed command to its service and sends the result to `output`.
//! This is the only place that knows about both the CLI types and the services.

use std::io;
use std::sync::Arc;

use btc_node::{CoreRpcBackend, RpcConfig, RpcOptions};
use clap::CommandFactory;

use crate::app::{self, AppError, Context, FeeConfig, FeeSource};
use crate::cli::{
    AddressCmd, BlockCmd, Cli, Command, FeeCmd, FeeMode, FeeSourceArg, KeyCmd, MnemonicCmd,
    MultisigCmd, NodeCmd, PsbtCmd, TxCmd,
};
use crate::output::{OutputMode, emit};

pub fn output_mode(cli: &Cli) -> OutputMode {
    if cli.global.json {
        OutputMode::Json
    } else {
        OutputMode::Human
    }
}

pub fn context(cli: &Cli) -> Result<Context, AppError> {
    let network = cli.global.network.into();
    let rpc = &cli.global.rpc;

    // Checked here rather than with clap's `requires`/`conflicts_with`, which ignore
    // values that come from environment variables (e.g. BTC_RPC_PASSWORD).
    if rpc.rpc_cookie.is_some() && rpc.rpc_user.is_some() {
        return Err(AppError::Config(
            "use either cookie auth or --rpc-user/--rpc-password, not both".into(),
        ));
    }
    if let Some(key) = &rpc.rpc_api_key {
        if key.trim().is_empty() {
            return Err(AppError::Config(
                "BTC_RPC_API_KEY is empty: paste your key into .env (BTC_RPC_API_KEY=...), \
                 or comment the line out to use a local node"
                    .into(),
            ));
        }
        if rpc.rpc_cookie.is_some() || rpc.rpc_user.is_some() {
            return Err(AppError::Config(
                "use either BTC_RPC_API_KEY or cookie / user-password auth, not both".into(),
            ));
        }
        let Some(url) = &rpc.rpc_url else {
            return Err(AppError::Config(
                "BTC_RPC_API_KEY needs BTC_RPC_URL (the gateway endpoint)".into(),
            ));
        };
        let local = ["//localhost", "//127.0.0.1", "//[::1]"]
            .iter()
            .any(|h| url.contains(h));
        if !url.starts_with("https://") && !local {
            return Err(AppError::Config(
                "refusing to send the API key over plain http; use an https:// URL".into(),
            ));
        }
    }
    match (&rpc.rpc_user, &rpc.rpc_password) {
        (Some(_), None) => {
            return Err(AppError::Config(
                "--rpc-user needs --rpc-password (or BTC_RPC_PASSWORD)".into(),
            ));
        }
        (None, Some(_)) => {
            return Err(AppError::Config(
                "--rpc-password needs --rpc-user (or BTC_RPC_USER)".into(),
            ));
        }
        _ => {}
    }

    let options = RpcOptions {
        url: rpc.rpc_url.clone(),
        cookie: rpc.rpc_cookie.clone(),
        user: rpc.rpc_user.clone(),
        password: rpc.rpc_password.clone(),
        api_key: rpc.rpc_api_key.clone(),
    };

    let rpc_config = RpcConfig::resolve(network, options);

    let backend =
        Some(Arc::new(CoreRpcBackend::new(&rpc_config)?) as Arc<dyn btc_node::NodeBackend>);

    let fees = FeeConfig {
        source: match cli.global.fees.fee_source {
            FeeSourceArg::Node => FeeSource::Node,
            FeeSourceArg::Public => FeeSource::Public,
            FeeSourceArg::Blocks => FeeSource::Blocks,
        },
        api_url: cli
            .global
            .fees
            .fee_api_url
            .clone()
            .unwrap_or_else(|| btc_node::fee_source::DEFAULT_PUBLIC_URL.to_owned()),
        blocks: usize::from(cli.global.fees.fee_blocks),
    };

    Ok(Context {
        network,
        fees,
        output: output_mode(cli),
        rpc: rpc_config,
        backend,
    })
}

pub fn run(cli: Cli, ctx: &Context) -> Result<(), AppError> {
    match cli.command {
        Command::Config => emit(&app::config::show(ctx), ctx.output),

        Command::Completions { shell } => {
            clap_complete::generate(shell, &mut Cli::command(), "btc", &mut io::stdout());
            Ok(())
        }

        Command::Key(KeyCmd::Generate { key_type }) => {
            emit(&app::keys::generate(ctx, key_type.into()), ctx.output)
        }
        Command::Mnemonic(MnemonicCmd::New { words, passphrase }) => emit(
            &app::mnemonic::new(ctx, words, passphrase.as_deref())?,
            ctx.output,
        ),
        Command::Mnemonic(MnemonicCmd::ToXprv {
            mnemonic,
            passphrase,
        }) => emit(
            &app::mnemonic::to_xprv(ctx, &mnemonic, passphrase.as_deref())?,
            ctx.output,
        ),
        Command::Derive(args) => emit(
            &app::derive::derive(
                ctx,
                &args.key,
                &args.path,
                args.count,
                args.addr_type.map(Into::into),
            )?,
            ctx.output,
        ),
        Command::Address(AddressCmd::FromPubkey { pubkey, addr_type }) => emit(
            &app::address::from_pubkey(ctx, &pubkey, addr_type.selected())?,
            ctx.output,
        ),
        Command::Address(AddressCmd::Validate { address }) => {
            emit(&app::address::validate(ctx, &address)?, ctx.output)
        }
        Command::Tx(TxCmd::Decode {
            hex,
            prevouts_from_node,
        }) => {
            let mode = if prevouts_from_node {
                app::tx::PrevoutMode::Required
            } else {
                app::tx::PrevoutMode::Off
            };
            emit(&app::tx::decode(ctx, &hex, mode)?, ctx.output)
        }

        Command::Node(NodeCmd::Status) => emit(&app::node::status(ctx)?, ctx.output),

        Command::Block(BlockCmd::Info { block }) => {
            emit(&app::node::block_info(ctx, &block)?, ctx.output)
        }

        Command::Fee(FeeCmd::Estimate {
            target,
            mode,
            fallback_rate,
            block,
            blocks,
        }) => {
            let mode_str = match mode {
                FeeMode::Economical => "economical",
                FeeMode::Conservative => "conservative",
            };
            if block.is_some() || blocks.is_some() {
                return emit(
                    &app::node::fee_estimate_from_blocks(ctx, block, blocks, Some(target))?,
                    ctx.output,
                );
            }
            emit(
                &app::node::fee_estimate(ctx, Some(target), Some(mode_str), fallback_rate)?,
                ctx.output,
            )
        }

        Command::Tx(TxCmd::Create {
            inputs,
            outputs,
            change,
            fee_rate,
        }) => emit(
            &app::tx::create(ctx, &inputs, &outputs, change.as_deref(), fee_rate)?,
            ctx.output,
        ),
        Command::Tx(TxCmd::Sign {
            psbt,
            key,
            key_file,
        }) => emit(
            &app::tx::sign(
                ctx,
                &psbt,
                key.as_deref(),
                key_file.as_ref().and_then(|p| p.to_str()),
            )?,
            ctx.output,
        ),
        Command::Tx(TxCmd::Broadcast { hex, yes }) => {
            emit(&app::tx::broadcast(ctx, &hex, yes)?, ctx.output)
        }

        Command::Psbt(PsbtCmd::Analyze { psbt }) => {
            emit(&app::psbt::analyze(ctx, &psbt)?, ctx.output)
        }
        Command::Psbt(PsbtCmd::Combine { psbts }) => {
            emit(&app::psbt::combine(ctx, &psbts)?, ctx.output)
        }

        Command::Multisig(MultisigCmd::Create {
            threshold,
            pubkeys,
            keep_order,
        }) => emit(
            &app::multisig::create(ctx, threshold, &pubkeys, keep_order)?,
            ctx.output,
        ),
        Command::Multisig(MultisigCmd::Analyze { script }) => {
            emit(&app::multisig::analyze(ctx, &script)?, ctx.output)
        }

        Command::Tui => {
            crate::tuiapp::run(ctx).map_err(|e| AppError::Input(format!("TUI failed: {e}")))?;
            Ok(())
        }
    }
}

#[allow(dead_code)]
fn not_implemented(command: &'static str) -> Result<(), AppError> {
    Err(AppError::NotImplemented(command))
}
