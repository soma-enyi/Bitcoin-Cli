//! Paying from a named wallet: find its coins, pick enough of them, build the PSBT and
//! describe it. Signing and broadcasting reuse `tx sign` and `tx broadcast`.

use bitcoin::psbt::Psbt;
use bitcoin::{ScriptBuf, Txid};

use btc_core::tx::build::{TxInput, TxOutput, create_psbt, parse_address};

use super::wallets::Wallet;
use super::{AppError, Context};

/// What `prepare` built, ready to show for review.
pub struct SendPlan {
    pub psbt: String,
    pub summary: String,
}

/// A coinbase output cannot be spent until it is 100 blocks deep; anything else can.
pub fn is_spendable(utxo: &btc_node::Utxo, tip: u32) -> bool {
    !utxo.coinbase || tip.saturating_sub(utxo.height) + 1 >= 100
}

pub fn parse_amount_sats(input: &str) -> Result<u64, AppError> {
    let sats: u64 =
        input.trim().replace('_', "").parse().map_err(|_| {
            AppError::Input("the amount is a whole number of sats, e.g. 25000".into())
        })?;
    if sats == 0 {
        return Err(AppError::Input("the amount cannot be zero".into()));
    }
    if sats > 21_000_000 * 100_000_000 {
        return Err(AppError::Input(
            "that is more than the 21 million BTC that can exist".into(),
        ));
    }
    Ok(sats)
}

/// Builds an unsigned payment of `amount_sats` to `to` from `wallet`'s coins, with the change
/// returned to the wallet. `fee_rate` of `None` asks the node (target 6 blocks).
pub fn prepare(
    ctx: &Context,
    wallet: &Wallet,
    to: &str,
    amount_sats: u64,
    fee_rate: Option<f64>,
) -> Result<SendPlan, AppError> {
    let recipient = parse_address(to, ctx.network)
        .map_err(|e| AppError::Input(format!("the recipient address is not valid: {e}")))?;
    let own = parse_address(&wallet.address, ctx.network)?;
    super::node::ensure_chain(ctx)?;
    let backend = ctx
        .backend
        .as_ref()
        .ok_or(AppError::NotImplemented("node backend"))?;

    let (fee_rate, fee_origin) = match fee_rate {
        Some(rate) if rate > 0.0 && rate.is_finite() => (rate, "your rate".to_owned()),
        Some(_) => return Err(AppError::Input("the fee rate must be positive".into())),
        None => {
            let estimate = super::node::fee_estimate(ctx, Some(6), None, None)?;
            (estimate.sat_vb, estimate.source)
        }
    };

    let tip = backend.node_status()?.blocks;
    let mut utxos = backend.list_utxos(&wallet.address)?;
    // A coinbase output (mined straight to this address) cannot be spent until 100 blocks deep:
    // the node would refuse the payment.
    let found = utxos.len();
    utxos.retain(|u| is_spendable(u, tip));
    if utxos.is_empty() && found > 0 {
        return Err(AppError::Input(format!(
            "wallet `{}` only has newly mined coins ({found}), which cannot be spent until they \
             are 100 blocks deep",
            wallet.name
        )));
    }
    if utxos.is_empty() {
        let hint = if ctx.network == btc_core::Network::Regtest {
            " On regtest, press Ctrl+O (or m on a tab without a text field) to mine coins to this wallet."
        } else {
            ""
        };
        return Err(AppError::Input(format!(
            "no coins found for wallet `{}` ({}). Fund it first; a payment already in the \
             mempool does not count until it confirms.{hint}",
            wallet.name, wallet.address
        )));
    }
    // Largest first keeps the number of inputs, and so the fee, small.
    utxos.sort_by(|a, b| b.sats.cmp(&a.sats));

    let script = own.script_pubkey();
    let mut inputs = Vec::new();
    let mut last_error = None;
    for utxo in &utxos {
        let txid: Txid = utxo
            .txid
            .parse()
            .map_err(|_| AppError::Input(format!("the node returned a bad txid {}", utxo.txid)))?;
        inputs.push(TxInput {
            txid,
            vout: utxo.vout,
            amount_sats: Some(utxo.sats),
            script_pubkey: Some(script.clone()),
        });
        let outputs = vec![TxOutput {
            address: recipient.clone(),
            amount_sats,
        }];
        match create_psbt(inputs.clone(), outputs, Some(own.clone()), fee_rate) {
            Ok(psbt) => {
                let summary = summarize(&psbt, wallet, to, &script, (fee_rate, &fee_origin));
                return Ok(SendPlan {
                    psbt: psbt.to_string(),
                    summary,
                });
            }
            // Not enough yet: add the next coin and try again.
            Err(e) if e.to_string().contains("insufficient funds") => last_error = Some(e),
            Err(e) => return Err(e.into()),
        }
    }
    let have: u64 = utxos.iter().map(|u| u.sats).sum();
    let reason = last_error.map(|e| e.to_string()).unwrap_or_default();
    Err(AppError::Input(format!(
        "wallet `{}` holds {have} sats in {} coin(s), not enough to send {amount_sats} plus the \
         fee ({reason})",
        wallet.name,
        utxos.len()
    )))
}

fn summarize(
    psbt: &Psbt,
    wallet: &Wallet,
    to: &str,
    own_script: &ScriptBuf,
    (rate, origin): (f64, &str),
) -> String {
    let total_in: u64 = psbt
        .inputs
        .iter()
        .filter_map(|i| i.witness_utxo.as_ref())
        .map(|o| o.value.to_sat())
        .sum();
    let outputs = &psbt.unsigned_tx.output;
    let total_out: u64 = outputs.iter().map(|o| o.value.to_sat()).sum();
    let change: u64 = outputs
        .iter()
        .filter(|o| &o.script_pubkey == own_script)
        .map(|o| o.value.to_sat())
        .sum();
    format!(
        "From wallet `{}`\nPay      {} sats\nTo       {to}\nFee      {} sats ({rate:.2} sat/vB, from {origin})\n\
         Change   {change} sats back to the wallet\nInputs   {}\nTotal    {total_in} sats in",
        wallet.name,
        total_out.saturating_sub(change),
        total_in.saturating_sub(total_out),
        psbt.inputs.len(),
    )
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;
    use std::sync::Arc;

    use bitcoin::{CompressedPublicKey, KnownHrp};
    use btc_core::Network;
    use btc_core::keys::{KeyType, generate, parse_private_key};
    use btc_node::{MockBackend, RpcConfig, RpcOptions};

    use super::*;
    use crate::app::FeeConfig;
    use crate::output::OutputMode;

    struct Setup {
        ctx: Context,
        wallet: Wallet,
        wif: String,
    }

    fn setup() -> Setup {
        let key = generate(Network::Regtest, KeyType::Ecdsa);
        let wif = key.private_key_wif.to_string();
        let secp = bitcoin::secp256k1::Secp256k1::new();
        let public = CompressedPublicKey(parse_private_key(&wif).unwrap().inner.public_key(&secp));
        let address = bitcoin::Address::p2wpkh(&public, KnownHrp::Regtest).to_string();
        Setup {
            ctx: Context {
                network: Network::Regtest,
                output: OutputMode::Human,
                rpc: RpcConfig::resolve(Network::Regtest, RpcOptions::default()),
                backend: Some(Arc::new(MockBackend)),
                fees: FeeConfig::default(),
            },
            wallet: Wallet {
                name: "alice".into(),
                address,
            },
            wif,
        }
    }

    const TO: &str = "bcrt1qw508d6qejxtdg4y5r3zarvary0c5xw7kygt080";

    #[test]
    fn prepares_a_payment_that_the_wallet_key_can_sign() {
        let s = setup();
        // The mock wallet holds 1,000,000 and 50,000 sats: the larger coin alone is enough.
        let plan = prepare(&s.ctx, &s.wallet, TO, 25_000, Some(2.0)).unwrap();
        assert!(
            plan.summary.contains("Pay      25000 sats"),
            "{}",
            plan.summary
        );
        assert!(plan.summary.contains("Inputs   1"), "{}", plan.summary);

        let signed = crate::app::tx::sign(&s.ctx, &plan.psbt, Some(&s.wif), None).unwrap();
        assert_eq!(signed.txid.len(), 64);
        let psbt = Psbt::from_str(&plan.psbt).unwrap();
        assert_eq!(psbt.unsigned_tx.output[0].value.to_sat(), 25_000);
    }

    #[test]
    fn uses_more_coins_when_one_is_not_enough_and_says_when_all_are_not() {
        let s = setup();
        let plan = prepare(&s.ctx, &s.wallet, TO, 1_030_000, Some(1.0)).unwrap();
        assert!(plan.summary.contains("Inputs   2"), "{}", plan.summary);
        let err = prepare(&s.ctx, &s.wallet, TO, 2_000_000, Some(1.0))
            .err()
            .unwrap()
            .to_string();
        assert!(err.contains("not enough"), "{err}");
    }

    /// A node whose coins are exactly the ones given, at a tip of 150.
    struct Coins(Vec<btc_node::Utxo>);

    impl btc_node::NodeBackend for Coins {
        fn node_status(&self) -> btc_node::backend::Result<btc_node::NodeStatus> {
            let mut status = MockBackend.node_status()?;
            status.blocks = 150;
            Ok(status)
        }
        fn block_info(&self, h: &str) -> btc_node::backend::Result<btc_node::BlockInfo> {
            MockBackend.block_info(h)
        }
        fn fee_estimate(
            &self,
            t: u16,
            m: &str,
        ) -> btc_node::backend::Result<btc_node::FeeEstimate> {
            MockBackend.fee_estimate(t, m)
        }
        fn list_utxos(&self, _: &str) -> btc_node::backend::Result<Vec<btc_node::Utxo>> {
            Ok(self.0.clone())
        }
        fn send_raw_transaction(&self, h: &str) -> btc_node::backend::Result<String> {
            MockBackend.send_raw_transaction(h)
        }
    }

    fn coin(byte: &str, sats: u64, height: u32, coinbase: bool) -> btc_node::Utxo {
        btc_node::Utxo {
            txid: byte.repeat(32),
            vout: 0,
            sats,
            height,
            coinbase,
        }
    }

    #[test]
    fn newly_mined_coinbase_coins_are_not_spent_until_100_blocks_deep() {
        let mut s = setup();
        // Tip 150: a coinbase at height 100 has 51 confirmations (immature), one at 50 has 101.
        s.ctx.backend = Some(Arc::new(Coins(vec![coin("aa", 5_000_000, 100, true)])));
        let err = prepare(&s.ctx, &s.wallet, TO, 1000, Some(1.0))
            .err()
            .unwrap()
            .to_string();
        assert!(err.contains("100 blocks deep"), "{err}");

        // The big immature coin is skipped and the small ordinary one is used.
        s.ctx.backend = Some(Arc::new(Coins(vec![
            coin("aa", 5_000_000, 100, true),
            coin("bb", 100_000, 120, false),
        ])));
        let plan = prepare(&s.ctx, &s.wallet, TO, 1000, Some(1.0)).unwrap();
        assert!(
            plan.summary.contains("Total    100000 sats in"),
            "{}",
            plan.summary
        );

        // Exactly 100 confirmations is mature (tip 150, height 51).
        s.ctx.backend = Some(Arc::new(Coins(vec![coin("cc", 100_000, 51, true)])));
        assert!(prepare(&s.ctx, &s.wallet, TO, 1000, Some(1.0)).is_ok());
        s.ctx.backend = Some(Arc::new(Coins(vec![coin("cc", 100_000, 52, true)])));
        assert!(prepare(&s.ctx, &s.wallet, TO, 1000, Some(1.0)).is_err());
    }

    #[test]
    fn an_empty_regtest_wallet_is_told_how_to_mine_coins() {
        let mut s = setup();
        s.ctx.backend = Some(Arc::new(Coins(Vec::new())));
        let err = prepare(&s.ctx, &s.wallet, TO, 1000, Some(1.0))
            .err()
            .unwrap()
            .to_string();
        assert!(
            err.contains("no coins found") && err.contains("Ctrl+O"),
            "{err}"
        );
    }

    #[test]
    fn a_huge_amount_is_refused_without_overflowing() {
        let s = setup();
        assert!(parse_amount_sats("99999999999999999999").is_err());
        assert!(parse_amount_sats("2100000000000001").is_err());
        let err = prepare(&s.ctx, &s.wallet, TO, u64::MAX, Some(1.0));
        assert!(err.is_err());
    }

    #[test]
    fn rejects_bad_input_before_touching_the_node() {
        let s = setup();
        assert!(prepare(&s.ctx, &s.wallet, "nonsense", 1000, Some(1.0)).is_err());
        assert!(prepare(&s.ctx, &s.wallet, TO, 1000, Some(0.0)).is_err());
        assert!(parse_amount_sats("0").is_err());
        assert!(parse_amount_sats("1.5").is_err());
        assert_eq!(parse_amount_sats("25_000").unwrap(), 25_000);
    }

    /// The whole payment against a real regtest node. Needs `BTC_REGTEST_URL`, `BTC_REGTEST_USER`,
    /// `BTC_REGTEST_PASS`, and `BTC_REGTEST_CLI`: a `bitcoin-cli` command (with -regtest, the rpc
    /// settings and `-rpcwallet=<a funded wallet>`) used to fund and mine. Run with
    /// `cargo test -p btc -- --ignored real_node`.
    #[test]
    #[ignore = "needs a regtest node: see the doc comment"]
    fn real_node_wallet_receives_pays_and_gets_change() {
        use btc_node::CoreRpcBackend;

        let (Ok(url), Ok(user), Ok(pass), Ok(cli)) = (
            std::env::var("BTC_REGTEST_URL"),
            std::env::var("BTC_REGTEST_USER"),
            std::env::var("BTC_REGTEST_PASS"),
            std::env::var("BTC_REGTEST_CLI"),
        ) else {
            eprintln!("BTC_REGTEST_* not set: skipped");
            return;
        };
        let run = |args: &[&str]| -> String {
            let mut parts = cli.split_whitespace();
            let out = std::process::Command::new(parts.next().unwrap())
                .args(parts)
                .args(args)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{args:?}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            String::from_utf8(out.stdout).unwrap().trim().to_owned()
        };
        let options = || btc_node::RpcOptions {
            url: Some(url.clone()),
            user: Some(user.clone()),
            password: Some(pass.clone()),
            ..btc_node::RpcOptions::default()
        };
        let ctx = Context {
            network: Network::Regtest,
            output: OutputMode::Human,
            rpc: RpcConfig::resolve(Network::Regtest, options()),
            backend: Some(Arc::new(
                CoreRpcBackend::new(&RpcConfig::resolve(Network::Regtest, options())).unwrap(),
            )),
            fees: FeeConfig::default(),
        };
        let backend = ctx.backend.clone().unwrap();

        let new = crate::app::wallets::generate(Network::Regtest, 24).unwrap();
        let wallet = Wallet {
            name: "e2e".into(),
            address: new.address.clone(),
        };
        let miner = run(&["getnewaddress"]);
        let recipient = run(&["getnewaddress"]);

        // Funded but unconfirmed: the UTXO set does not have it yet.
        run(&["sendtoaddress", &wallet.address, "0.01"]);
        let err = prepare(&ctx, &wallet, &recipient, 300_000, Some(2.0))
            .err()
            .expect("unconfirmed funds must not be spendable")
            .to_string();
        assert!(err.contains("no coins found"), "{err}");

        run(&["generatetoaddress", "1", &miner]);
        let coins = backend.list_utxos(&wallet.address).unwrap();
        assert_eq!(coins.len(), 1, "{coins:?}");
        assert_eq!(coins[0].sats, 1_000_000);

        let plan = prepare(&ctx, &wallet, &recipient, 300_000, Some(2.0)).unwrap();
        let wif = crate::app::wallets::wif_from_phrase(Network::Regtest, &new.phrase).unwrap();
        let signed = crate::app::tx::sign(&ctx, &plan.psbt, Some(&wif), None).unwrap();
        let sent = crate::app::tx::broadcast(&ctx, &signed.hex, false).unwrap();
        assert_eq!(sent.txid, signed.txid);
        run(&["generatetoaddress", "1", &miner]);

        assert_eq!(run(&["getreceivedbyaddress", &recipient]), "0.00300000");
        let change = backend.list_utxos(&wallet.address).unwrap();
        assert_eq!(change.len(), 1, "{change:?}");
        let fee = 1_000_000 - 300_000 - change[0].sats;
        assert!(
            (100..=600).contains(&fee),
            "fee {fee} sats at 2 sat/vB, one input"
        );
    }

    /// A real regtest node and a way to drive it, or `None` (test skipped) without `BTC_REGTEST_*`.
    fn real_node() -> Option<(Context, String)> {
        use btc_node::CoreRpcBackend;
        let (url, user, pass, cli) = (
            std::env::var("BTC_REGTEST_URL").ok()?,
            std::env::var("BTC_REGTEST_USER").ok()?,
            std::env::var("BTC_REGTEST_PASS").ok()?,
            std::env::var("BTC_REGTEST_CLI").ok()?,
        );
        let options = || btc_node::RpcOptions {
            url: Some(url.clone()),
            user: Some(user.clone()),
            password: Some(pass.clone()),
            ..btc_node::RpcOptions::default()
        };
        let ctx = Context {
            network: Network::Regtest,
            output: OutputMode::Human,
            rpc: RpcConfig::resolve(Network::Regtest, options()),
            backend: Some(Arc::new(
                CoreRpcBackend::new(&RpcConfig::resolve(Network::Regtest, options())).unwrap(),
            )),
            fees: FeeConfig::default(),
        };
        Some((ctx, cli))
    }

    fn cli_run(cli: &str, args: &[&str]) -> String {
        let mut parts = cli.split_whitespace();
        let out = std::process::Command::new(parts.next().unwrap())
            .args(parts)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap().trim().to_owned()
    }

    /// Pays at 1 sat/vB with one and with three inputs and checks the node's own accounting:
    /// the real fee rate (fee / vsize in the mempool) must not fall below 1.
    #[test]
    #[ignore = "needs a regtest node: see the doc comment of the test above"]
    fn real_node_fee_meets_the_requested_rate_with_one_and_three_inputs() {
        let Some((ctx, cli)) = real_node() else {
            eprintln!("BTC_REGTEST_* not set: skipped");
            return;
        };
        let run = |args: &[&str]| cli_run(&cli, args);
        let miner = run(&["getnewaddress"]);
        let recipient = run(&["getnewaddress"]);

        for (label, coins) in [("1 input", vec!["0.01"]), ("3 inputs", vec!["0.0004"; 3])] {
            let new = crate::app::wallets::generate(Network::Regtest, 24).unwrap();
            let wallet = Wallet {
                name: "vsize".into(),
                address: new.address.clone(),
            };
            for amount in &coins {
                run(&["sendtoaddress", &wallet.address, amount]);
            }
            run(&["generatetoaddress", "1", &miner]);
            // Spends nearly everything, so every coin is needed.
            let pay = if coins.len() == 1 { 300_000 } else { 100_000 };
            let plan = prepare(&ctx, &wallet, &recipient, pay, Some(1.0)).unwrap();
            let wif = crate::app::wallets::wif_from_phrase(Network::Regtest, &new.phrase).unwrap();
            let signed = crate::app::tx::sign(&ctx, &plan.psbt, Some(&wif), None).unwrap();
            let sent = crate::app::tx::broadcast(&ctx, &signed.hex, false).unwrap();

            let entry: serde_json::Value =
                serde_json::from_str(&run(&["getmempoolentry", &sent.txid])).unwrap();
            let vsize = entry["vsize"].as_f64().unwrap();
            let fee_sats = (entry["fees"]["base"].as_f64().unwrap() * 1e8).round();
            let rate = fee_sats / vsize;
            eprintln!(
                "{label}: fee {fee_sats} sats, real vsize {vsize} vB, so {rate:.3} sat/vB \
                 (estimated vsize at 1 sat/vB = fee {fee_sats})"
            );
            assert!(
                rate >= 1.0,
                "{label}: {rate} sat/vB is below the requested 1.0"
            );
            assert!(
                rate < 1.1,
                "{label}: {rate} sat/vB overpays by more than 10%"
            );
            run(&["generatetoaddress", "1", &miner]);
        }
    }

    /// Coins mined straight to the wallet's address: refused until 100 blocks deep, then accepted
    /// by the node.
    #[test]
    #[ignore = "needs a regtest node: see the doc comment of the test above"]
    fn real_node_coinbase_coins_wait_for_100_blocks() {
        let Some((ctx, cli)) = real_node() else {
            eprintln!("BTC_REGTEST_* not set: skipped");
            return;
        };
        let run = |args: &[&str]| cli_run(&cli, args);
        let miner = run(&["getnewaddress"]);
        let recipient = run(&["getnewaddress"]);
        let new = crate::app::wallets::generate(Network::Regtest, 24).unwrap();
        let wallet = Wallet {
            name: "cb".into(),
            address: new.address.clone(),
        };

        run(&["generatetoaddress", "1", &wallet.address]);
        // 99 confirmations after 98 more blocks: still immature.
        run(&["generatetoaddress", "98", &miner]);
        let err = prepare(&ctx, &wallet, &recipient, 1_000_000, Some(2.0))
            .err()
            .expect("99 confirmations is immature")
            .to_string();
        assert!(err.contains("100 blocks deep"), "{err}");

        run(&["generatetoaddress", "1", &miner]);
        let plan = prepare(&ctx, &wallet, &recipient, 1_000_000, Some(2.0)).unwrap();
        let wif = crate::app::wallets::wif_from_phrase(Network::Regtest, &new.phrase).unwrap();
        let signed = crate::app::tx::sign(&ctx, &plan.psbt, Some(&wif), None).unwrap();
        // The node itself agrees the coin is spendable.
        crate::app::tx::broadcast(&ctx, &signed.hex, false).unwrap();
        run(&["generatetoaddress", "1", &miner]);
    }
}
