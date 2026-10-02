//! Per-network node settings.
//!
//! `BTC_NETWORK=mainnet` should pick the mainnet node and `BTC_NETWORK=regtest` the regtest
//! one, from a single `.env`. Settings for a network are `BTC_<NETWORK>_<NAME>`, e.g.
//! `BTC_MAINNET_RPC_URL`. When any of them exist for the selected network they *replace*
//! the plain `BTC_RPC_*` variables, so a mainnet API key can never leak into a regtest run.

const NAMES: [&str; 5] = [
    "RPC_URL",
    "RPC_COOKIE",
    "RPC_USER",
    "RPC_PASSWORD",
    "RPC_API_KEY",
];

/// Which network the command line or environment selects, before clap parses anything.
pub fn selected_network(args: &[String], env_network: Option<String>) -> String {
    let mut iter = args.iter().skip(1);
    while let Some(arg) = iter.next() {
        if arg == "--" {
            break;
        }
        if arg == "-n" || arg == "--network" {
            if let Some(value) = iter.next() {
                return value.to_lowercase();
            }
        } else if let Some(value) = arg.strip_prefix("--network=") {
            return value.to_lowercase();
        }
    }
    env_network
        .map(|v| v.trim().to_lowercase())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "regtest".to_owned())
}

/// Settings that are not credentials: a network-specific value simply overrides the plain one.
const SIMPLE_NAMES: [&str; 3] = ["FEE_SOURCE", "FEE_API_URL", "FEE_BLOCKS"];

/// What to do to the environment: `(variable, Some(value))` sets it, `(variable, None)` removes it.
pub fn plan(network: &str, get: impl Fn(&str) -> Option<String>) -> Vec<(String, Option<String>)> {
    let prefix = format!("BTC_{}_", network.to_uppercase());
    let mut actions = Vec::new();

    let specific: Vec<Option<String>> =
        NAMES.iter().map(|n| get(&format!("{prefix}{n}"))).collect();
    if specific.iter().any(Option::is_some) {
        actions.extend(
            NAMES
                .iter()
                .zip(specific)
                .map(|(name, value)| (format!("BTC_{name}"), value)),
        );
    }
    for name in SIMPLE_NAMES {
        if let Some(value) = get(&format!("{prefix}{name}")) {
            actions.push((format!("BTC_{name}"), Some(value)));
        }
    }
    actions
}

/// Applies [`plan`] to the real environment. Call once, at startup, before any thread exists.
pub fn apply(args: &[String]) {
    let network = selected_network(args, std::env::var("BTC_NETWORK").ok());
    for (name, value) in plan(&network, |k| std::env::var(k).ok()) {
        // SAFETY: called first thing in `main`, while the process is still single-threaded.
        unsafe {
            match value {
                Some(v) => std::env::set_var(&name, v),
                None => std::env::remove_var(&name),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn flag_beats_environment_and_defaults_to_regtest() {
        assert_eq!(
            selected_network(
                &args(&["btc", "-n", "Mainnet", "node"]),
                Some("signet".into())
            ),
            "mainnet"
        );
        assert_eq!(
            selected_network(&args(&["btc", "--network=signet"]), None),
            "signet"
        );
        assert_eq!(
            selected_network(&args(&["btc", "node"]), Some("mainnet".into())),
            "mainnet"
        );
        assert_eq!(selected_network(&args(&["btc", "node"]), None), "regtest");
        assert_eq!(
            selected_network(&args(&["btc", "node"]), Some("  ".into())),
            "regtest"
        );
    }

    #[test]
    fn a_network_section_replaces_the_plain_variables() {
        let env: HashMap<&str, &str> = HashMap::from([
            ("BTC_MAINNET_RPC_URL", "https://gw"),
            ("BTC_MAINNET_RPC_API_KEY", "k"),
            ("BTC_RPC_COOKIE", "/old/cookie"),
            ("BTC_REGTEST_RPC_URL", "http://local"),
        ]);
        let get = |k: &str| env.get(k).map(|v| v.to_string());
        let main: HashMap<_, _> = plan("mainnet", get).into_iter().collect();
        assert_eq!(main["BTC_RPC_URL"].as_deref(), Some("https://gw"));
        assert_eq!(main["BTC_RPC_API_KEY"].as_deref(), Some("k"));
        assert_eq!(
            main["BTC_RPC_COOKIE"], None,
            "generic cookie must be cleared"
        );
        let reg: HashMap<_, _> = plan("regtest", get).into_iter().collect();
        assert_eq!(reg["BTC_RPC_URL"].as_deref(), Some("http://local"));
        assert_eq!(
            reg["BTC_RPC_API_KEY"], None,
            "mainnet key must not reach regtest"
        );
    }

    #[test]
    fn fee_settings_override_one_by_one_without_touching_the_node_settings() {
        let env: HashMap<&str, &str> = HashMap::from([
            ("BTC_MAINNET_FEE_SOURCE", "public"),
            ("BTC_RPC_URL", "http://keep-me"),
            ("BTC_FEE_API_URL", "https://generic"),
        ]);
        let get = |k: &str| env.get(k).map(|v| v.to_string());
        let actions: HashMap<_, _> = plan("mainnet", get).into_iter().collect();
        assert_eq!(actions["BTC_FEE_SOURCE"].as_deref(), Some("public"));
        assert!(
            !actions.contains_key("BTC_RPC_URL"),
            "node settings untouched"
        );
        assert!(!actions.contains_key("BTC_FEE_API_URL"));
    }

    #[test]
    fn without_a_section_the_plain_variables_are_left_alone() {
        let get = |k: &str| (k == "BTC_RPC_URL").then(|| "http://x".to_owned());
        assert!(plan("signet", get).is_empty());
    }
}
