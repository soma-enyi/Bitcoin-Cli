use std::path::PathBuf;

use btc_core::Network;
use directories::BaseDirs;
use zeroize::Zeroizing;

/// Connection settings supplied by the user (flags or environment). Anything left
/// as `None` falls back to Bitcoin Core's defaults for the selected network.
#[derive(Default)]
pub struct RpcOptions {
    pub url: Option<String>,
    pub cookie: Option<PathBuf>,
    pub user: Option<String>,
    pub password: Option<String>,
    /// Key for an authenticated gateway, sent as the `X-API-Key` header.
    pub api_key: Option<String>,
}

/// Fully resolved connection settings.
pub struct RpcConfig {
    pub url: String,
    pub auth: RpcAuth,
}

/// How we authenticate to the node. Deliberately not `Debug`, so the password
/// can never end up in logs or panic messages.
pub enum RpcAuth {
    /// Bitcoin Core writes a fresh `.cookie` file each time it starts.
    Cookie(PathBuf),
    /// A hosted gateway that authenticates with the `X-API-Key` header.
    ApiKey(Zeroizing<String>),
    UserPass {
        user: String,
        password: Zeroizing<String>,
    },
}

impl RpcConfig {
    pub fn resolve(network: Network, options: RpcOptions) -> Self {
        let url = options
            .url
            .unwrap_or_else(|| format!("http://127.0.0.1:{}", default_rpc_port(network)));

        let auth = match (options.api_key, options.user, options.password) {
            (Some(key), _, _) => RpcAuth::ApiKey(Zeroizing::new(key)),
            (None, Some(user), Some(password)) => RpcAuth::UserPass {
                user,
                password: Zeroizing::new(password),
            },
            _ => RpcAuth::Cookie(
                options
                    .cookie
                    .unwrap_or_else(|| default_cookie_path(network)),
            ),
        };

        RpcConfig { url, auth }
    }
}

/// Bitcoin Core's default RPC port for each network.
pub fn default_rpc_port(network: Network) -> u16 {
    match network {
        Network::Mainnet => 8332,
        Network::Regtest => 18443,
        Network::Signet => 38332,
        Network::Testnet4 => 48332,
    }
}

/// Where Bitcoin Core writes its cookie file by default, e.g. `~/.bitcoin/regtest/.cookie`.
pub fn default_cookie_path(network: Network) -> PathBuf {
    // Mainnet's data directory is the root; every other chain has a subdirectory.
    if network.is_mainnet() {
        return default_data_dir().join(".cookie");
    }
    default_data_dir().join(network.as_str()).join(".cookie")
}

fn default_data_dir() -> PathBuf {
    let Some(dirs) = BaseDirs::new() else {
        return PathBuf::from(".bitcoin");
    };
    if cfg!(target_os = "linux") {
        dirs.home_dir().join(".bitcoin")
    } else {
        // macOS: ~/Library/Application Support/Bitcoin, Windows: %APPDATA%\Bitcoin
        dirs.data_dir().join("Bitcoin")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_follow_the_network() {
        let config = RpcConfig::resolve(Network::Signet, RpcOptions::default());
        assert_eq!(config.url, "http://127.0.0.1:38332");
        match config.auth {
            RpcAuth::Cookie(path) => assert!(path.ends_with("signet/.cookie")),
            _ => panic!("expected cookie auth"),
        }
    }

    #[test]
    fn explicit_options_override_defaults() {
        let options = RpcOptions {
            url: Some("http://10.0.0.5:18443".into()),
            user: Some("alice".into()),
            password: Some("secret".into()),
            ..RpcOptions::default()
        };
        let config = RpcConfig::resolve(Network::Regtest, options);
        assert_eq!(config.url, "http://10.0.0.5:18443");
        assert!(matches!(config.auth, RpcAuth::UserPass { ref user, .. } if user == "alice"));
    }

    #[test]
    fn mainnet_defaults() {
        let config = RpcConfig::resolve(Network::Mainnet, RpcOptions::default());
        assert_eq!(config.url, "http://127.0.0.1:8332");
        match config.auth {
            RpcAuth::Cookie(path) => assert!(path.ends_with(".bitcoin/.cookie")),
            _ => panic!("expected cookie auth"),
        }
    }

    #[test]
    fn api_key_takes_over_authentication() {
        let options = RpcOptions {
            url: Some("https://gateway.example/bitcoin".into()),
            api_key: Some("k".into()),
            ..RpcOptions::default()
        };
        let config = RpcConfig::resolve(Network::Regtest, options);
        assert!(matches!(config.auth, RpcAuth::ApiKey(_)));
    }
}
