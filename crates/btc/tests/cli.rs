//! End-to-end tests: run the real `btc` binary and check stdout, stderr and exit codes.

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;

/// The binary with a clean environment, so the developer's own BTC_* settings
/// can't change test results.
fn btc() -> Command {
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("btc");
    for var in [
        "BTC_NETWORK",
        "BTC_RPC_URL",
        "BTC_RPC_COOKIE",
        "BTC_RPC_USER",
        "BTC_RPC_PASSWORD",
        "BTC_RPC_API_KEY",
    ] {
        cmd.env_remove(var);
    }
    // dotenvy searches parent directories, so run somewhere a stray .env can't be found.
    cmd.current_dir(std::env::temp_dir());
    cmd
}

fn stdout_json(cmd: &mut Command) -> Value {
    let output = cmd.output().unwrap();
    serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON")
}

#[test]
fn help_lists_every_command_group() {
    let output = btc().arg("--help").output().unwrap();
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    for group in [
        "key", "mnemonic", "derive", "address", "tx", "block", "fee", "node", "config", "tui",
    ] {
        assert!(
            help.contains(&format!("  {group} ")),
            "missing `{group}` in --help"
        );
    }
}

#[test]
fn config_uses_network_default_port() {
    let json = stdout_json(btc().args(["--network", "signet", "config", "--json"]));
    assert_eq!(json["network"], "signet");
    assert_eq!(json["rpc_url"], "http://127.0.0.1:38332");
    assert_eq!(json["rpc_auth"]["method"], "cookie");
}

#[test]
fn network_can_come_from_the_environment() {
    let json = stdout_json(
        btc()
            .env("BTC_NETWORK", "testnet4")
            .args(["config", "--json"]),
    );
    assert_eq!(json["rpc_url"], "http://127.0.0.1:48332");
}

#[test]
fn config_never_shows_the_password() {
    btc()
        .args(["config", "--json", "--rpc-user", "alice"])
        .env("BTC_RPC_PASSWORD", "hunter2")
        .assert()
        .success()
        .stdout(predicate::str::contains("alice"))
        .stdout(predicate::str::contains("hunter2").not());
}

#[test]
fn rpc_user_without_password_is_a_config_error() {
    let mut cmd = btc();
    cmd.args(["config", "--json", "--rpc-user", "alice"]);
    cmd.assert().code(1);
    assert_eq!(stdout_json(&mut cmd)["error"]["kind"], "invalid_config");
}

#[test]
fn json_errors_are_printed_to_stdout() {
    let v = stdout_json(btc().args(["--json", "block", "info", "zz"]));
    assert_eq!(v["error"]["kind"], "invalid_input");
}








#[test]
fn usage_errors_exit_with_code_1_not_clap_default_2() {
    btc().args(["tx", "sign", "cHNidP8="]).assert().code(1);
}

// ---- Phase 2: keys and mnemonics ----

const ABANDON_ABOUT: &str =
    "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

#[test]
fn key_generate_gives_testnet_wif_and_compressed_pubkey() {
    let json = stdout_json(btc().args(["key", "generate", "--json"]));
    assert_eq!(json["key_type"], "ecdsa");
    assert!(json["private_key_wif"].as_str().unwrap().starts_with('c'));
    let pubkey = json["public_key"].as_str().unwrap();
    assert_eq!(pubkey.len(), 66);
    assert!(pubkey.starts_with("02") || pubkey.starts_with("03"));
}

#[test]
fn schnorr_key_is_x_only_with_parity() {
    let json = stdout_json(btc().args(["key", "generate", "--type", "schnorr", "--json"]));
    assert_eq!(json["public_key"].as_str().unwrap().len(), 64);
    assert!(json["parity"] == "even" || json["parity"] == "odd");
}

#[test]
fn secrets_trigger_a_warning_on_stderr_only_in_human_mode() {
    btc()
        .args(["key", "generate"])
        .assert()
        .success()
        .stdout(predicate::str::contains("warning").not())
        .stderr(predicate::str::contains(
            "warning: this output contains secret key material",
        ));
    btc()
        .args(["key", "generate", "--json"])
        .assert()
        .success()
        .stderr(predicate::str::is_empty());
}

#[test]
fn mnemonic_new_respects_word_count() {
    let json = stdout_json(btc().args(["mnemonic", "new", "--words", "24", "--json"]));
    assert_eq!(json["mnemonic"].as_str().unwrap().split(' ').count(), 24);
    assert_eq!(json["entropy_bits"], 256);
    assert_eq!(json["checksum_bits"], 8);
    assert!(json["root"]["xprv"].as_str().unwrap().starts_with("tprv"));
}

#[test]
fn mnemonic_to_xprv_reads_words_from_stdin() {
    // 73c5da0a is the well-known master fingerprint of this mnemonic (BIP84 test vector).
    let json = stdout_json(
        btc()
            .args(["mnemonic", "to-xprv", "-", "--json"])
            .write_stdin(format!("{ABANDON_ABOUT}\n")),
    );
    assert_eq!(json["fingerprint"], "73c5da0a");
}

#[test]
fn passphrase_changes_the_fingerprint() {
    let json = stdout_json(btc().args([
        "mnemonic",
        "to-xprv",
        ABANDON_ABOUT,
        "--passphrase",
        "TREZOR",
        "--json",
    ]));
    assert_ne!(json["fingerprint"], "73c5da0a");
}

#[test]
fn invalid_mnemonic_is_an_input_error() {
    let mut cmd = btc();
    cmd.args(["mnemonic", "to-xprv", &["abandon"; 12].join(" "), "--json"]);
    cmd.assert().code(1);
    assert_eq!(stdout_json(&mut cmd)["error"]["kind"], "invalid_input");
}

#[test]
fn only_one_argument_may_come_from_stdin() {
    let mut cmd = btc();
    cmd.args(["mnemonic", "to-xprv", "-", "--passphrase", "-", "--json"]);
    cmd.assert().code(1);
    assert_eq!(stdout_json(&mut cmd)["error"]["kind"], "invalid_input");
}

// ---- Phase 3: derivation ----

fn abandon_tprv() -> String {
    let json = stdout_json(btc().args(["mnemonic", "to-xprv", ABANDON_ABOUT, "--json"]));
    json["xprv"].as_str().unwrap().to_owned()
}

#[test]
fn derive_infers_address_type_from_the_path() {
    let json = stdout_json(btc().args(["derive", &abandon_tprv(), "m/86'/1'/0'/0/0", "--json"]));
    assert_eq!(json["address_type"], "p2tr");
    assert_eq!(json["address_type_source"], "path_purpose");
    assert!(
        json["children"][0]["address"]
            .as_str()
            .unwrap()
            .starts_with("bcrt1p")
    );
}

#[test]
fn derive_uses_the_network_for_address_encoding() {
    let tprv = abandon_tprv();
    let regtest = stdout_json(btc().args(["derive", &tprv, "m/84'/1'/0'/0/0", "--json"]));
    let signet =
        stdout_json(btc().args(["-n", "signet", "derive", &tprv, "m/84'/1'/0'/0/0", "--json"]));
    assert!(
        regtest["children"][0]["address"]
            .as_str()
            .unwrap()
            .starts_with("bcrt1q")
    );
    assert!(
        signet["children"][0]["address"]
            .as_str()
            .unwrap()
            .starts_with("tb1q")
    );
}

#[test]
fn derive_count_lists_consecutive_children() {
    let json = stdout_json(btc().args([
        "derive",
        &abandon_tprv(),
        "m/84'/1'/0'/0/0",
        "--count",
        "5",
        "--json",
    ]));
    let children = json["children"].as_array().unwrap();
    assert_eq!(children.len(), 5);
    assert_eq!(children[4]["path"], "m/84'/1'/0'/0/4");
}

#[test]
fn derive_reads_the_key_from_stdin() {
    let json = stdout_json(
        btc()
            .args(["derive", "-", "m/84'/1'/0'", "--json"])
            .write_stdin(abandon_tprv()),
    );
    assert!(
        json["children"][0]["xpub"]
            .as_str()
            .unwrap()
            .starts_with("tpub")
    );
}

#[test]
fn derive_from_tpub_rejects_hardened_paths_and_hides_secrets() {
    let account = stdout_json(btc().args(["derive", &abandon_tprv(), "m/84'/1'/0'", "--json"]));
    let tpub = account["children"][0]["xpub"].as_str().unwrap();

    let mut cmd = btc();
    cmd.args(["derive", tpub, "0'", "--json"]);
    cmd.assert().code(1);
    assert!(
        stdout_json(&mut cmd)["error"]["message"]
            .as_str()
            .unwrap()
            .contains("hardened")
    );

    // Public-only derivation: no private material, so no warning either.
    btc()
        .args(["derive", tpub, "0/0"])
        .assert()
        .success()
        .stdout(predicate::str::contains("xprv").not())
        .stderr(predicate::str::is_empty());
}

// ---- Phase 4: addresses ----

const G_COMPRESSED: &str = "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";

#[test]
fn from_pubkey_makes_all_four_types() {
    let json = stdout_json(btc().args(["address", "from-pubkey", G_COMPRESSED, "--json"]));
    let types: Vec<_> = json["addresses"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["address_type"].as_str().unwrap())
        .collect();
    assert_eq!(types, ["p2pkh", "p2sh-p2wpkh", "p2wpkh", "p2tr"]);
}

#[test]
fn from_pubkey_single_type() {
    let json = stdout_json(btc().args([
        "-n",
        "signet",
        "address",
        "from-pubkey",
        G_COMPRESSED,
        "--type",
        "p2wpkh",
        "--json",
    ]));
    assert_eq!(
        json["addresses"][0]["address"],
        "tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx"
    );
}

#[test]
fn every_generated_address_validates_on_its_network() {
    for network in ["regtest", "signet", "testnet4"] {
        let set = stdout_json(btc().args([
            "-n",
            network,
            "address",
            "from-pubkey",
            G_COMPRESSED,
            "--json",
        ]));
        for entry in set["addresses"].as_array().unwrap() {
            let address = entry["address"].as_str().unwrap();
            let report =
                stdout_json(btc().args(["-n", network, "address", "validate", address, "--json"]));
            assert_eq!(
                report["script_pubkey"], entry["script_pubkey"],
                "{network} {address}"
            );
        }
    }
}

#[test]
fn validate_rejects_a_bad_checksum() {
    let mut cmd = btc();
    cmd.args([
        "-n",
        "signet",
        "address",
        "validate",
        "tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsy",
        "--json",
    ]);
    cmd.assert().code(1);
    assert_eq!(stdout_json(&mut cmd)["error"]["kind"], "invalid_input");
}

#[test]
fn validate_reports_wrong_network() {
    let mut cmd = btc();
    cmd.args([
        "address",
        "validate",
        "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4",
        "--json",
    ]);
    cmd.assert().code(1);
    let json = stdout_json(&mut cmd);
    assert_eq!(json["error"]["kind"], "wrong_network");
    assert!(
        json["error"]["message"]
            .as_str()
            .unwrap()
            .contains("mainnet")
    );
}

// ---- Phase 5: transaction decoding ----

/// Real regtest transactions recorded from Bitcoin Core (see btc-core/testdata).
fn fixture_hex(name: &str) -> String {
    let fixtures: Value =
        serde_json::from_str(include_str!("../../btc-core/testdata/regtest_txs.json")).unwrap();
    fixtures[name]["hex"].as_str().unwrap().to_owned()
}

#[test]
fn tx_decode_reports_structure() {
    let json = stdout_json(btc().args(["tx", "decode", &fixture_hex("mixed_inputs"), "--json"]));
    assert_eq!(
        json["txid"],
        "b94b83b802022270f19785ff8d5cf0a92754df1c384be693a077a27902534acf"
    );
    assert_eq!(json["inputs"].as_array().unwrap().len(), 4);
    assert_eq!(json["rbf"], true);
    assert_eq!(json["segwit"], true);
    assert!(json.get("fee").is_none());
    assert_eq!(json["outputs"][1]["op_return"]["text"], "hello btc-cli");
}

#[test]
fn tx_decode_reads_hex_from_stdin() {
    let json = stdout_json(
        btc()
            .args(["tx", "decode", "-", "--json"])
            .write_stdin(format!("{}\n", fixture_hex("coinbase"))),
    );
    assert_eq!(json["coinbase"], true);
    assert_eq!(json["inputs"][0]["coinbase_height"], 105);
}

#[test]
fn tx_decode_of_a_txid_asks_the_node_and_reports_when_it_is_unreachable() {
    // A txid is no longer refused outright: the transaction is fetched from the node.
    let mut cmd = btc();
    cmd.args([
        "tx",
        "decode",
        "b94b83b802022270f19785ff8d5cf0a92754df1c384be693a077a27902534acf",
        "--json",
    ])
    .env("BTC_RPC_COOKIE", "/nonexistent/.cookie");
    cmd.assert().code(2);
    assert_eq!(stdout_json(&mut cmd)["error"]["kind"], "node");
}


const K1: &str = "02c6047f9441ed7d6d3045406e95c07cd85c778e4b8cef3ca7abac09b95c709ee5";
const K2: &str = "03a34b99f22c790c4e36b2b3c2c35a36db06226e41c692fc82b8b56ac1c540c5bd";
const K3: &str = "025cbdf0646e5db4eaa398f365f2ea7a0e3d419b7e0330e39ce92bddedcac4f9bc";

#[test]
fn block_info_rejects_a_bad_height_before_touching_the_node() {
    btc()
        .args(["block", "info", "99999999999"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("block height must be a number"));
}

#[test]
fn block_info_rejects_a_short_hash() {
    btc()
        .args(["block", "info", "abcdef"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("block hash must be 64 hex chars"));
}

#[test]
fn fee_target_out_of_range_is_rejected_by_clap() {
    btc()
        .args(["fee", "estimate", "--target", "5000"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("1..=1008"));
}

#[test]
fn unreachable_node_exits_2_with_a_hint() {
    btc()
        .args(["node", "status"])
        .env("BTC_RPC_COOKIE", "/nonexistent/.cookie")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("cannot read cookie file"))
        .stderr(predicate::str::contains("BTC_RPC_COOKIE"));
}

#[test]
fn unreachable_node_json_error_has_kind_node() {
    let v = stdout_json(
        btc()
            .args(["--json", "node", "status"])
            .env("BTC_RPC_COOKIE", "/nonexistent/.cookie"),
    );
    assert_eq!(v["error"]["kind"], "node");
}

#[test]
fn offline_commands_do_not_need_a_node() {
    btc()
        .args(["key", "generate"])
        .env("BTC_RPC_COOKIE", "/nonexistent/.cookie")
        .assert()
        .success();
}

#[test]
fn dotenv_file_in_the_working_directory_is_loaded() {
    let dir = std::env::temp_dir().join(format!("btc-dotenv-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(".env"), "BTC_NETWORK=signet\n").unwrap();
    let v = stdout_json(btc().current_dir(&dir).args(["--json", "config"]));
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(v["network"], "signet");
}

#[test]
fn real_environment_beats_dotenv() {
    let dir = std::env::temp_dir().join(format!("btc-dotenv-env-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(".env"), "BTC_NETWORK=signet\n").unwrap();
    let v = stdout_json(
        btc()
            .current_dir(&dir)
            .env("BTC_NETWORK", "testnet4")
            .args(["--json", "config"]),
    );
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(v["network"], "testnet4");
}

#[test]
fn multisig_create_matches_bitcoin_core_addresses() {
    // Verified against `bitcoin-cli createmultisig 2 [K1,K2,K3]` on regtest (keys in given order).
    let v = stdout_json(btc().args([
        "--json", "multisig", "create", "--threshold", "2", "--keep-order", "--pubkey", K1, K2, K3,
    ]));
    assert_eq!(v["p2sh"], "2MxQa1EbxeEcn365ze92PyDeaYRQTfg1K9m");
    assert_eq!(
        v["p2wsh"],
        "bcrt1q3x2mlpmfr6vngwc05qseq25vxt46wpzvfgj3wn30egcj36zkgzuqtyt4wz"
    );
    assert_eq!(v["p2sh_p2wsh"], "2MytgBjeyDgnvriweusDGBir3gvfgi4fs3u");
}

#[test]
fn multisig_create_rejects_bad_threshold_and_keys() {
    btc()
        .args(["multisig", "create", "--threshold", "5", "--pubkey", K1, K2])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("threshold must be between 1"));
    btc()
        .args(["multisig", "create", "--threshold", "1", "--pubkey", "nothex"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("bad public key"));
}

#[test]
fn multisig_analyze_round_trips_a_created_script() {
    let created = stdout_json(btc().args([
        "--json", "multisig", "create", "--threshold", "2", "--pubkey", K1, K2, K3,
    ]));
    let script = created["script_hex"].as_str().unwrap().to_owned();
    let v = stdout_json(btc().args(["--json", "multisig", "analyze", &script]));
    assert_eq!(v["threshold"], 2);
    assert_eq!(v["total_keys"], 3);
}

#[test]
fn psbt_analyze_rejects_garbage() {
    btc()
        .args(["psbt", "analyze", "notapsbt"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("invalid PSBT"));
}

#[test]
fn psbt_combine_needs_two_psbts() {
    btc().args(["psbt", "combine", "cHNidP8="]).assert().failure();
}

#[test]
fn api_key_requires_a_url() {
    btc()
        .args(["node", "status"])
        .env("BTC_RPC_API_KEY", "k")
        .assert()
        .code(1)
        .stderr(predicate::str::contains("BTC_RPC_API_KEY needs BTC_RPC_URL"));
}

#[test]
fn api_key_is_never_sent_over_plain_http_to_a_remote_host() {
    btc()
        .args(["node", "status"])
        .env("BTC_RPC_API_KEY", "k")
        .env("BTC_RPC_URL", "http://example.com/bitcoin")
        .assert()
        .code(1)
        .stderr(predicate::str::contains("plain http"));
}

#[test]
fn api_key_cannot_be_mixed_with_other_auth() {
    btc()
        .args(["node", "status"])
        .env("BTC_RPC_API_KEY", "k")
        .env("BTC_RPC_URL", "https://x")
        .env("BTC_RPC_USER", "a")
        .assert()
        .code(1)
        .stderr(predicate::str::contains("not both"));
}

#[test]
fn blank_api_key_gets_a_helpful_message() {
    btc()
        .args(["node", "status"])
        .env("BTC_RPC_API_KEY", "")
        .env("BTC_RPC_URL", "https://x")
        .assert()
        .code(1)
        .stderr(predicate::str::contains("BTC_RPC_API_KEY is empty"));
}

#[test]
fn config_shows_api_key_auth_but_never_the_key() {
    let output = btc()
        .args(["--json", "config"])
        .env("BTC_RPC_API_KEY", "super-secret-key")
        .env("BTC_RPC_URL", "https://gateway.example/bitcoin")
        .output()
        .unwrap();
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("api_key"));
    assert!(!text.contains("super-secret-key"));
}

/// One-shot HTTP server: replies like a gateway that wants `X-API-Key: secret-key`.
fn fake_gateway() -> String {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/bitcoin", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming().take(2) {
            let mut stream = stream.unwrap();
            let mut buf = [0u8; 8192];
            let n = stream.read(&mut buf).unwrap();
            let request = String::from_utf8_lossy(&buf[..n]).to_lowercase();
            let reply = if !request.contains("x-api-key: secret-key") {
                "HTTP/1.1 401 Unauthorized\r\nContent-Length: 2\r\n\r\n{}".to_owned()
            } else {
                let body = if request.contains("getnetworkinfo") {
                    r#"{"result":{"connections":10},"error":null,"id":1}"#
                } else {
                    r#"{"result":{"chain":"main","blocks":900000,"headers":900000,"verificationprogress":0.5},"error":null,"id":1}"#
                };
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
            };
            stream.write_all(reply.as_bytes()).unwrap();
        }
    });
    url
}

#[test]
fn node_status_sends_the_api_key_header_to_a_gateway() {
    let url = fake_gateway();
    let v = stdout_json(
        btc()
            .args(["--json", "-n", "mainnet", "node", "status"])
            .env("BTC_RPC_URL", &url)
            .env("BTC_RPC_API_KEY", "secret-key"),
    );
    assert_eq!(v["chain"], "main");
    assert_eq!(v["blocks"], 900000);
    assert_eq!(v["connections"], 10);
}

#[test]
fn a_rejected_api_key_exits_2_and_names_the_variable() {
    let url = fake_gateway();
    btc()
        .args(["node", "status"])
        .env("BTC_RPC_URL", &url)
        .env("BTC_RPC_API_KEY", "wrong")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("BTC_RPC_API_KEY"));
}

#[test]
fn mainnet_is_accepted_for_config() {
    let v = stdout_json(btc().args(["--json", "--network", "mainnet", "config"]));
    assert_eq!(v["network"], "mainnet");
}

#[test]
fn mainnet_defaults_to_the_mainnet_rpc_port() {
    let v = stdout_json(btc().args(["--json", "-n", "mainnet", "config"]));
    assert_eq!(v["rpc_url"], "http://127.0.0.1:8332");
}

const ABANDON: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
const ABANDON_MAINNET_XPRV: &str = "xprv9s21ZrQH143K3GJpoapnV8SFfukcVBSfeCficPSGfubmSFDxo1kuHnLisriDvSnRRuL2Qrg5ggqHKNVpxR86QEC8w35uxmGoggxtQTPvfUu";

#[test]
fn mainnet_mnemonic_gives_the_bip39_xprv() {
    let v = stdout_json(btc().args(["--json", "-n", "mainnet", "mnemonic", "to-xprv", ABANDON]));
    assert_eq!(v["xprv"], ABANDON_MAINNET_XPRV);
}

#[test]
fn mainnet_derive_matches_the_bip_vectors() {
    for (path, expected) in [
        ("m/44'/0'/0'/0/0", "1LqBGSKuX5yYUonjxT5qGfpUsXKYYWeabA"),
        ("m/49'/0'/0'/0/0", "37VucYSaXLCAsxYyAPfbSi9eh4iEcbShgf"),
        ("m/84'/0'/0'/0/0", "bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu"),
    ] {
        let v = stdout_json(btc().args([
            "--json", "-n", "mainnet", "derive", ABANDON_MAINNET_XPRV, path,
        ]));
        assert_eq!(v["children"][0]["address"], expected, "{path}");
    }
}

#[test]
fn a_mainnet_key_is_refused_on_regtest() {
    btc()
        .args(["-n", "regtest", "derive", ABANDON_MAINNET_XPRV, "m/84'/0'/0'/0/0"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("mainnet key"))
        .stderr(predicate::str::contains("--network mainnet"));
}

#[test]
fn mainnet_key_generation_warns_loudly_on_stderr() {
    let output = btc().args(["-n", "mainnet", "key", "generate"]).output().unwrap();
    assert!(output.status.success());
    let out = String::from_utf8(output.stdout).unwrap();
    assert!(out.contains("mainnet"));
    let wif = out
        .lines()
        .find(|l| l.starts_with("private key (WIF)"))
        .and_then(|l| l.split_whitespace().last())
        .unwrap();
    assert!(wif.starts_with('K') || wif.starts_with('L'), "{wif}");
    let err = String::from_utf8(output.stderr).unwrap();
    assert!(err.contains("MAINNET"), "{err}");
}

#[test]
fn test_network_keys_keep_the_regular_warning() {
    let output = btc().args(["-n", "regtest", "key", "generate"]).output().unwrap();
    let err = String::from_utf8(output.stderr).unwrap();
    assert!(err.contains("secret key material") && !err.contains("MAINNET"), "{err}");
}

#[test]
fn mainnet_address_validation_is_network_specific() {
    let addr = "bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu";
    btc().args(["-n", "mainnet", "address", "validate", addr]).assert().success();
    btc().args(["-n", "regtest", "address", "validate", addr]).assert().code(1);
}

#[test]
fn mainnet_multisig_makes_mainnet_addresses() {
    let v = stdout_json(btc().args([
        "--json", "-n", "mainnet", "multisig", "create", "--threshold", "2", "--pubkey", K1, K2, K3,
    ]));
    assert!(v["p2sh"].as_str().unwrap().starts_with('3'));
    assert!(v["p2wsh"].as_str().unwrap().starts_with("bc1q"));
}

#[test]
fn mainnet_node_queries_reach_the_gateway() {
    let url = fake_gateway();
    let v = stdout_json(
        btc()
            .args(["--json", "--network", "mainnet", "node", "status"])
            .env("BTC_RPC_URL", &url)
            .env("BTC_RPC_API_KEY", "secret-key"),
    );
    assert_eq!(v["chain"], "main");
}

#[test]
fn unknown_network_lists_the_valid_ones() {
    btc()
        .args(["--network", "bitcoin", "config"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("mainnet"));
}

/// A real signed regtest transaction, built and signed by the binary (no node needed).
fn signed_regtest_tx() -> Value {
    let zero = "0000000000000000000000000000000000000000000000000000000000000000";
    let key = stdout_json(btc().args(["--json", "-n", "regtest", "key", "generate"]));
    let wif = key["private_key_wif"].as_str().unwrap().to_owned();
    let pubkey = key["public_key"].as_str().unwrap().to_owned();
    let addrs = stdout_json(btc().args(["--json", "-n", "regtest", "address", "from-pubkey", &pubkey]));
    let address = addrs["addresses"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["address"].as_str().unwrap().to_owned())
        .find(|a| a.starts_with("bcrt1q"))
        .unwrap();
    let psbt = String::from_utf8(
        btc()
            .args([
                "-n", "regtest", "tx", "create",
                "--input", &format!("{zero}:0:100000:{address}"),
                "--to", &format!("{address}:60000"),
                "--change", &address,
                "--fee-rate", "2",
            ])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    stdout_json(btc().args(["--json", "-n", "regtest", "tx", "sign", psbt.trim(), "--key", &wif]))
}

#[test]
fn tx_create_then_sign_produces_a_decodable_witness_transaction() {
    let signed = signed_regtest_tx();
    let hex = signed["hex"].as_str().unwrap();
    assert!(hex.starts_with("02000000000101"), "segwit marker and flag expected: {hex}");
    let decoded = stdout_json(btc().args(["--json", "-n", "regtest", "tx", "decode", hex]));
    assert_eq!(decoded["txid"], signed["txid"]);
}

#[test]
fn broadcasting_on_mainnet_needs_an_explicit_yes() {
    let signed = signed_regtest_tx();
    btc()
        .args(["-n", "mainnet", "tx", "broadcast", signed["hex"].as_str().unwrap()])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("MAINNET"))
        .stderr(predicate::str::contains("--yes"));
}

#[test]
fn tx_sign_refuses_a_key_from_the_wrong_network() {
    let main = stdout_json(btc().args(["--json", "-n", "mainnet", "key", "generate"]));
    let wif = main["private_key_wif"].as_str().unwrap();
    btc()
        .args(["-n", "regtest", "tx", "sign", "cHNidP8=", "--key", wif])
        .assert()
        .code(1);
}

#[test]
fn per_network_variables_select_the_node_and_replace_the_plain_ones() {
    let url = fake_gateway();
    let v = stdout_json(
        btc()
            .args(["--json", "node", "status"])
            .env("BTC_NETWORK", "mainnet")
            .env("BTC_MAINNET_RPC_URL", &url)
            .env("BTC_MAINNET_RPC_API_KEY", "secret-key")
            .env("BTC_RPC_URL", "http://127.0.0.1:1/unused")
            .env("BTC_RPC_COOKIE", "/nonexistent/.cookie"),
    );
    assert_eq!(v["chain"], "main");
}

#[test]
fn a_mainnet_key_never_reaches_another_network() {
    let v = stdout_json(
        btc()
            .args(["--json", "config"])
            .env("BTC_NETWORK", "regtest")
            .env("BTC_MAINNET_RPC_URL", "https://gateway.example/bitcoin")
            .env("BTC_MAINNET_RPC_API_KEY", "super-secret-key")
            .env("BTC_REGTEST_RPC_URL", "http://127.0.0.1:18443"),
    );
    assert_eq!(v["rpc_auth"]["method"], "cookie");
    assert_eq!(v["rpc_url"], "http://127.0.0.1:18443");
}

#[test]
fn a_node_on_the_wrong_chain_is_refused() {
    let url = fake_gateway();
    btc()
        .args(["node", "status"])
        .env("BTC_NETWORK", "regtest")
        .env("BTC_REGTEST_RPC_URL", &url)
        .env("BTC_REGTEST_RPC_API_KEY", "secret-key")
        .assert()
        .code(1)
        .stderr(predicate::str::contains("the node is on chain `main`"))
        .stderr(predicate::str::contains("BTC_REGTEST_RPC_URL"));
}

const GENESIS_COINBASE: &str = "01000000010000000000000000000000000000000000000000000000000000000000000000ffffffff4d04ffff001d0104455468652054696d65732030332f4a616e2f32303039204368616e63656c6c6f72206f6e206272696e6b206f66207365636f6e64206261696c6f757420666f722062616e6b73ffffffff0100f2052a01000000434104678afdb0fe5548271967f1a67130b7105cd6a828e03909a67962e0ea1f61deb649f6bc3f4cef38c4f35504e51ec112de5c384df7ba0b8d578a4c702b6bf11d5fac00000000";
const GENESIS_TXID: &str = "4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b";

/// A node that answers by RPC method name. `raw` is the JSON object `getrawtransaction`
/// returns (verbosity 2), or None for "no such mempool transaction" (a node without txindex).
fn fake_node(raw: Option<String>) -> String {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/bitcoin", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming().take(6) {
            let mut stream = stream.unwrap();
            let mut buf = [0u8; 8192];
            let n = stream.read(&mut buf).unwrap();
            let request = String::from_utf8_lossy(&buf[..n]).to_string();
            let (status, body) = if request.contains("getblockchaininfo") {
                ("200 OK", r#"{"result":{"chain":"main"},"error":null,"id":1}"#.to_owned())
            } else if request.contains("getblockhash") {
                ("200 OK", format!(r#"{{"result":"{}","error":null,"id":1}}"#, "ab".repeat(32)))
            } else {
                match &raw {
                    Some(object) => ("200 OK", format!(r#"{{"result":{object},"error":null,"id":1}}"#)),
                    None => (
                        "404 Not Found",
                        r#"{"result":null,"error":{"code":-5,"message":"No such mempool transaction. Use -txindex or provide a block hash"},"id":1}"#.to_owned(),
                    ),
                }
            };
            let reply = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(reply.as_bytes()).unwrap();
        }
    });
    url
}

fn coinbase_object() -> String {
    format!(r#"{{"hex":"{GENESIS_COINBASE}","vin":[{{"coinbase":"04ffff001d0104"}}]}}"#)
}

/// A mainnet taproot spend (block 840000) and the amounts of what it spends, as a node reports them.
const SPEND_HEX: &str = "020000000001017fb9cc941aa0ca3aaf339783564d2d29ec3254a9128f5d49ad3eeb002aeb40ec0000000000000000000242342a6b000000002251203b8b3ab1453eb47e2d4903b963776680e30863df3625d3e74292338ae7928da10000000000000000246a5d21020704b5e1d8e1c8eeb788a30705a02d039f3e01020680dc9afd2808c7e8430a640340924b2624416402a52ed7cf4eba6b2c535d2def8e649a74ed97aaca5ec54881ef3b34da68bb13d76d6b420e60297a9247cb081d1e59cb2c260b1509cff25d4b3158204c04e894d5357840e324b24c959ca6a5082035f6ffae12f331202bc84bf4612eac0063036f7264010b2047f22ed15d3082f5e9a005864528e4f991ade841a9c5846e2c118425878b6be1010d09b530368c74df10a3036821c04c04e894d5357840e324b24c959ca6a5082035f6ffae12f331202bc84bf4612e00000000";

fn spend_object() -> String {
    let script = format!("5120{}", "ab".repeat(32));
    format!(
        r#"{{"hex":"{SPEND_HEX}","vin":[{{"txid":"x","vout":0,"prevout":{{"value":24.71128002,"scriptPubKey":{{"hex":"{script}"}}}}}}]}}"#
    )
}

#[test]
fn tx_decode_by_txid_and_block_also_computes_the_fee_from_the_nodes_amounts() {
    let url = fake_node(Some(spend_object()));
    let v = stdout_json(
        btc()
            .args(["--json", "-n", "mainnet", "tx", "decode", &format!("{}@840000", "2b".repeat(32))])
            .env("BTC_RPC_URL", &url)
            .env("BTC_RPC_USER", "u")
            .env("BTC_RPC_PASSWORD", "p"),
    );
    // 24.71128002 BTC in, 17.97928002 BTC out.
    assert_eq!(v["fee"]["sats"], 673_200_000u64);
}

#[test]
fn prevouts_from_node_fills_in_the_fee_for_pasted_hex_when_the_node_can_say() {
    let url = fake_node(Some(spend_object()));
    let v = stdout_json(
        btc()
            .args(["--json", "-n", "mainnet", "tx", "decode", SPEND_HEX, "--prevouts-from-node"])
            .env("BTC_RPC_URL", &url)
            .env("BTC_RPC_USER", "u")
            .env("BTC_RPC_PASSWORD", "p"),
    );
    assert_eq!(v["fee"]["sats"], 673_200_000u64);
}

#[test]
fn prevouts_from_node_without_a_transaction_index_says_how_to_proceed() {
    let url = fake_node(None);
    btc()
        .args(["-n", "mainnet", "tx", "decode", SPEND_HEX, "--prevouts-from-node"])
        .env("BTC_RPC_URL", &url)
        .env("BTC_RPC_USER", "u")
        .env("BTC_RPC_PASSWORD", "p")
        .assert()
        .code(1)
        .stderr(predicate::str::contains("<txid>@<block height>"));
}

#[test]
fn without_the_flag_decoding_hex_never_touches_the_node() {
    let v = stdout_json(
        btc()
            .args(["--json", "-n", "mainnet", "tx", "decode", SPEND_HEX])
            .env("BTC_RPC_COOKIE", "/nonexistent/.cookie"),
    );
    assert!(v["fee"].is_null());
}

#[test]
fn tx_decode_fetches_a_transaction_by_txid_and_block_height() {
    let url = fake_node(Some(coinbase_object()));
    let v = stdout_json(
        btc()
            .args(["--json", "-n", "mainnet", "tx", "decode", &format!("{GENESIS_TXID}@5")])
            .env("BTC_RPC_URL", &url)
            .env("BTC_RPC_USER", "u")
            .env("BTC_RPC_PASSWORD", "p"),
    );
    assert_eq!(v["txid"], GENESIS_TXID);
}

#[test]
fn tx_decode_by_txid_alone_explains_what_to_add_when_the_node_has_no_index() {
    let url = fake_node(None);
    btc()
        .args(["-n", "mainnet", "tx", "decode", GENESIS_TXID])
        .env("BTC_RPC_URL", &url)
        .env("BTC_RPC_USER", "u")
        .env("BTC_RPC_PASSWORD", "p")
        .assert()
        .code(1)
        .stderr(predicate::str::contains("<txid>@<height or block hash>"));
}

#[test]
fn tx_decode_still_works_offline_for_raw_hex() {
    let v = stdout_json(btc().args(["--json", "-n", "mainnet", "tx", "decode", GENESIS_COINBASE]));
    assert_eq!(v["txid"], GENESIS_TXID);
}

#[test]
fn tx_decode_rejects_a_malformed_block_reference() {
    btc()
        .args(["tx", "decode", &format!("{GENESIS_TXID}@nope")])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("block height or a 64-character block hash"));
}

/// A node whose own fee estimator is blocked (like your gateway) but which serves blocks.
/// `chain` is what getblockchaininfo reports.
fn fake_fee_node(chain: &'static str) -> String {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/bitcoin", listener.local_addr().unwrap());
    // Two non-coinbase transactions: 10 sat/vB and 40 sat/vB, 500 vB each.
    let block = r#"{"tx":[{"vsize":200},{"fee":0.00005,"vsize":500},{"fee":0.0002,"vsize":500}]}"#;
    std::thread::spawn(move || {
        for stream in listener.incoming().take(40) {
            let mut stream = stream.unwrap();
            let mut buf = [0u8; 8192];
            let n = stream.read(&mut buf).unwrap();
            let request = String::from_utf8_lossy(&buf[..n]).to_string();
            let (status, body) = if request.contains("getblockchaininfo") {
                ("200 OK", format!(r#"{{"result":{{"chain":"{chain}"}},"error":null,"id":1}}"#))
            } else if request.contains("estimatesmartfee") {
                ("400 Bad Request", r#"{"error":"Bad Request","message":"Method \"estimatesmartfee\" is not permitted"}"#.to_owned())
            } else if request.contains("getmempoolinfo") {
                ("200 OK", r#"{"result":{"mempoolminfee":0.00001,"minrelaytxfee":0.00001},"error":null,"id":1}"#.to_owned())
            } else if request.contains("getblockcount") {
                ("200 OK", r#"{"result":100,"error":null,"id":1}"#.to_owned())
            } else if request.contains("getblockhash") {
                ("200 OK", format!(r#"{{"result":"{}","error":null,"id":1}}"#, "cd".repeat(32)))
            } else {
                ("200 OK", format!(r#"{{"result":{block},"error":null,"id":1}}"#))
            };
            let reply = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(reply.as_bytes()).unwrap();
        }
    });
    url
}

/// A fee service. Returns its URL and a counter of how many requests it received.
fn fake_fee_service(body: &'static str) -> (String, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
    use std::io::{Read, Write};
    use std::sync::atomic::{AtomicUsize, Ordering};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/api/fee-estimates", listener.local_addr().unwrap());
    let hits = std::sync::Arc::new(AtomicUsize::new(0));
    let counter = hits.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().take(10) {
            let mut stream = stream.unwrap();
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf).unwrap();
            counter.fetch_add(1, Ordering::SeqCst);
            let reply = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(reply.as_bytes()).unwrap();
        }
    });
    (url, hits)
}

fn fee_json(args: &[&str], envs: &[(&str, &str)]) -> Value {
    let mut cmd = btc();
    cmd.args(["--json", "fee", "estimate"]).args(args);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    stdout_json(&mut cmd)
}

#[test]
fn mainnet_public_source_gives_per_target_rates_that_fall_as_the_target_rises() {
    let node = fake_fee_node("main");
    let (api, _) = fake_fee_service(r#"{"1":20.0,"6":10.0,"144":0.25}"#);
    let envs = [
        ("BTC_RPC_URL", node.as_str()), ("BTC_RPC_USER", "u"), ("BTC_RPC_PASSWORD", "p"),
        ("BTC_FEE_SOURCE", "public"), ("BTC_FEE_API_URL", api.as_str()),
    ];
    let fast = fee_json(&["-n", "mainnet", "--target", "1"], &envs);
    let slow = fee_json(&["-n", "mainnet", "--target", "144"], &envs);
    assert_eq!(fast["sat_vb"], 20.0);
    assert_eq!(slow["sat_vb"], 1.0, "0.25 is raised to the 1 sat/vB relay minimum");
    assert!(fast["source"].as_str().unwrap().contains("public fee service"));
    assert!(fast["note"].as_str().unwrap().contains("IP address"));
}

#[test]
fn mainnet_blocks_source_estimates_from_recent_block_fee_rates() {
    let node = fake_fee_node("main");
    let envs = [
        ("BTC_RPC_URL", node.as_str()), ("BTC_RPC_USER", "u"), ("BTC_RPC_PASSWORD", "p"),
        ("BTC_FEE_SOURCE", "blocks"), ("BTC_FEE_BLOCKS", "1"),
    ];
    // Rates are 10 and 40 sat/vB with equal weight. A 1-block target uses the 25th percentile (10),
    // a long target the 1st (also 10): the cheapest rate that recently confirmed.
    let v = fee_json(&["-n", "mainnet", "--target", "1"], &envs);
    assert_eq!(v["sat_vb"], 10.0);
    assert!(v["source"].as_str().unwrap().contains("last 1 blocks"));
}

#[test]
fn without_a_source_mainnet_falls_back_to_the_labelled_floor_and_calls_nobody() {
    let node = fake_fee_node("main");
    let (api, hits) = fake_fee_service(r#"{"1":20.0}"#);
    let v = fee_json(
        &["-n", "mainnet"],
        &[("BTC_RPC_URL", &node), ("BTC_RPC_USER", "u"), ("BTC_RPC_PASSWORD", "p"), ("BTC_FEE_API_URL", &api)],
    );
    assert_eq!(v["source"], "the mempool minimum");
    assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 0, "no source configured: no external call");
}

#[test]
fn test_networks_never_use_an_external_fee_source() {
    let node = fake_fee_node("regtest");
    let (api, hits) = fake_fee_service(r#"{"1":20.0}"#);
    let v = fee_json(
        &["-n", "regtest"],
        &[("BTC_RPC_URL", &node), ("BTC_RPC_USER", "u"), ("BTC_RPC_PASSWORD", "p"),
          ("BTC_FEE_SOURCE", "public"), ("BTC_FEE_API_URL", &api)],
    );
    assert_eq!(v["source"], "the mempool minimum");
    assert!(v["note"].as_str().unwrap().contains("no fee data yet"), "explains a fresh chain");
    assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 0, "regtest must not call a public service");
}

#[test]
fn a_failing_fee_service_falls_back_to_the_floor_and_says_why() {
    let node = fake_fee_node("main");
    let v = fee_json(
        &["-n", "mainnet"],
        &[("BTC_RPC_URL", &node), ("BTC_RPC_USER", "u"), ("BTC_RPC_PASSWORD", "p"),
          ("BTC_FEE_SOURCE", "public"), ("BTC_FEE_API_URL", "http://example.com/not-https")],
    );
    assert_eq!(v["source"], "the mempool minimum");
    assert!(v["note"].as_str().unwrap().contains("public fee service failed"));
}

#[test]
fn the_fee_source_follows_the_selected_network_from_the_per_network_variable() {
    let node = fake_fee_node("main");
    let (api, _) = fake_fee_service(r#"{"1":20.0,"6":10.0}"#);
    let v = fee_json(
        &[],
        &[("BTC_NETWORK", "mainnet"), ("BTC_MAINNET_RPC_URL", &node), ("BTC_MAINNET_RPC_USER", "u"),
          ("BTC_MAINNET_RPC_PASSWORD", "p"), ("BTC_MAINNET_FEE_SOURCE", "public"), ("BTC_MAINNET_FEE_API_URL", &api)],
    );
    assert_eq!(v["sat_vb"], 10.0);
}
