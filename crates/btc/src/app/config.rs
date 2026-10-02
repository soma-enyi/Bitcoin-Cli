use std::path::PathBuf;

use btc_core::Network;
use btc_node::RpcAuth;
use serde::Serialize;

use super::Context;

/// The effective settings after flags, environment variables and defaults are applied.
#[derive(Serialize)]
pub struct ConfigView {
    pub network: Network,
    pub rpc_url: String,
    pub rpc_auth: AuthView,
}

#[derive(Serialize)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum AuthView {
    /// `exists` is false when bitcoind isn't running on this network: the node
    /// deletes its cookie on shutdown.
    Cookie { path: PathBuf, exists: bool },
    /// The password is never shown.
    UserPass { user: String },
    /// The key is never shown, only that one is set.
    ApiKey,
}

pub fn show(ctx: &Context) -> ConfigView {
    let rpc_auth = match &ctx.rpc.auth {
        RpcAuth::Cookie(path) => AuthView::Cookie {
            exists: path.exists(),
            path: path.clone(),
        },
        RpcAuth::UserPass { user, .. } => AuthView::UserPass { user: user.clone() },
        RpcAuth::ApiKey(_) => AuthView::ApiKey,
    };

    ConfigView {
        network: ctx.network,
        rpc_url: ctx.rpc.url.clone(),
        rpc_auth,
    }
}
