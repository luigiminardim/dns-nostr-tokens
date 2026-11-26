use crate::bitcoin_token_tx::RegtestTokenClient;
use crate::config::E2eConfig;
use bitcoin::{Address, Amount, Denomination, Network};
use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    str::FromStr,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

const BIP39_VECTOR: &str =
    "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
const REGTEST_EXTERNAL_0: &str = "bcrt1q6rz28mcfaxtmd6v789l9rrlrusdprr9pz3cppk";
const REGTEST_EXTERNAL_1: &str = "bcrt1qd7spv5q28348xl4myc8zmh983w5jx32cs707jh";

struct TempWalletDir(PathBuf);

impl TempWalletDir {
    fn new(test_name: &str) -> Result<Self, String> {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("system clock before UNIX_EPOCH: {error}"))?
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "dns_nostr_wallet_e2e_{test_name}_{}_{}_{}",
            std::process::id(),
            nanos,
            seq
        ));
        std::fs::create_dir_all(&dir).map_err(|error| {
            format!(
                "failed to create temp wallet dir `{}`: {error}",
                dir.display()
            )
        })?;
        Ok(Self(dir))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempWalletDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub fn run_all(
    config: &E2eConfig,
    bitcoin_client: &RegtestTokenClient,
) -> Result<(), (&'static str, String)> {
    run_step(
        "wallet::create_generates_seed_sets_0600_and_rejects_duplicate",
        create_generates_seed_sets_0600_and_rejects_duplicate(config),
    )?;
    run_step(
        "wallet::import_args_and_address_auto_advance",
        import_args_and_address_auto_advance(config),
    )?;
    run_step(
        "wallet::import_via_stdin_and_address",
        import_via_stdin_and_address(config),
    )?;
    run_step(
        "wallet::network_env_overrides_and_invalid_network",
        network_env_overrides_and_invalid_network(config),
    )?;
    run_step(
        "wallet::error_paths_do_not_leak_seed_words",
        error_paths_do_not_leak_seed_words(config),
    )?;
    run_step(
        "wallet::receive_and_balance_tracking",
        receive_and_balance_tracking(config, bitcoin_client),
    )?;
    run_step(
        "wallet::send_and_consecutive_unconfirmed_change_spend",
        send_and_consecutive_unconfirmed_change_spend(config, bitcoin_client),
    )?;
    run_step(
        "wallet::historical_import_recovery",
        historical_import_recovery(config, bitcoin_client),
    )?;
    Ok(())
}

fn run_step(name: &'static str, result: Result<(), String>) -> Result<(), (&'static str, String)> {
    match result {
        Ok(()) => {
            println!("[PASS] {name}");
            Ok(())
        }
        Err(error) => Err((name, error)),
    }
}

fn run_wallet(
    config: &E2eConfig,
    wallet_dir: &Path,
    network: Option<&str>,
    args: &[&str],
    stdin_input: Option<&str>,
) -> Result<Output, String> {
    let binary = E2eConfig::wallet_binary_path()?;
    let mut command = Command::new(binary);
    command
        .args(args)
        .env("DNS_NOSTR_WALLET_DIR", wallet_dir)
        .env("DNS_NOSTR_BITCOIN_RPC_URL", &config.bitcoin_rpc_url)
        .env("DNS_NOSTR_BITCOIN_RPC_USER", &config.bitcoin_rpc_user)
        .env(
            "DNS_NOSTR_BITCOIN_RPC_PASSWORD",
            &config.bitcoin_rpc_password,
        )
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    match network {
        Some(value) => {
            command.env("DNS_NOSTR_NETWORK", value);
        }
        None => {
            command.env_remove("DNS_NOSTR_NETWORK");
        }
    }

    if let Some(input) = stdin_input {
        command.stdin(Stdio::piped());
        let mut child = command
            .spawn()
            .map_err(|error| format!("failed to spawn dns_nostr_wallet: {error}"))?;
        child
            .stdin
            .as_mut()
            .ok_or_else(|| "failed to open dns_nostr_wallet stdin".to_string())?
            .write_all(input.as_bytes())
            .map_err(|error| format!("failed to write stdin to dns_nostr_wallet: {error}"))?;
        child
            .wait_with_output()
            .map_err(|error| format!("failed to wait for dns_nostr_wallet: {error}"))
    } else {
        command
            .stdin(Stdio::null())
            .output()
            .map_err(|error| format!("failed to execute dns_nostr_wallet: {error}"))
    }
}

fn run_wallet_ok(
    config: &E2eConfig,
    wallet_dir: &Path,
    args: &[&str],
    stdin_input: Option<&str>,
) -> Result<String, String> {
    let output = run_wallet(config, wallet_dir, None, args, stdin_input)?;
    if !output.status.success() {
        return Err(format!(
            "wallet {:?} failed: stderr={}",
            args,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn parse_regtest_address(raw: &str) -> Result<Address, String> {
    Address::from_str(raw)
        .map_err(|error| format!("invalid address `{raw}`: {error}"))?
        .require_network(Network::Regtest)
        .map_err(|error| format!("address `{raw}` is not regtest: {error}"))
}

fn parse_btc_amount(raw: &str) -> Result<Amount, String> {
    Amount::from_str_in(raw, Denomination::Bitcoin)
        .map_err(|error| format!("invalid BTC amount `{raw}`: {error}"))
}

fn create_generates_seed_sets_0600_and_rejects_duplicate(config: &E2eConfig) -> Result<(), String> {
    let dir = TempWalletDir::new("create")?;
    let first = run_wallet(config, dir.path(), None, &["wallet", "create"], None)?;
    if !first.status.success() {
        return Err(format!(
            "wallet create failed: stderr={}",
            String::from_utf8_lossy(&first.stderr)
        ));
    }

    let stdout = String::from_utf8_lossy(&first.stdout);
    let seed_line = stdout
        .lines()
        .last()
        .ok_or_else(|| format!("missing seed phrase line in stdout: {stdout}"))?
        .trim();
    if seed_line.split_whitespace().count() != 12 {
        return Err(format!(
            "expected 12-word BIP39 seed phrase, got: {stdout}"
        ));
    }

    let db_path = dir.path().join("wallet.sqlite");
    if !db_path.is_file() {
        return Err("wallet.sqlite should exist after create".to_string());
    }
    let db_bytes_before = std::fs::read(&db_path)
        .map_err(|error| format!("failed to read wallet.sqlite: {error}"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&db_path)
            .map_err(|error| format!("failed to stat wallet.sqlite: {error}"))?
            .permissions()
            .mode()
            & 0o777;
        if mode != 0o600 {
            return Err(format!(
                "wallet.sqlite must have 0600 permissions, got {:o}",
                mode
            ));
        }
    }

    let second = run_wallet(config, dir.path(), None, &["wallet", "create"], None)?;
    if second.status.success() {
        return Err("second wallet create should exit non-zero".to_string());
    }
    let db_bytes_after = std::fs::read(&db_path)
        .map_err(|error| format!("failed to re-read wallet.sqlite: {error}"))?;
    if db_bytes_before != db_bytes_after {
        return Err("rejected second wallet create must not alter wallet.sqlite".to_string());
    }
    Ok(())
}

fn import_args_and_address_auto_advance(config: &E2eConfig) -> Result<(), String> {
    let dir = TempWalletDir::new("import_args")?;
    let mut import_args = vec!["wallet", "import"];
    import_args.extend(BIP39_VECTOR.split_whitespace());

    let import = run_wallet(config, dir.path(), None, &import_args, None)?;
    if !import.status.success() {
        return Err(format!(
            "wallet import failed: stderr={}",
            String::from_utf8_lossy(&import.stderr)
        ));
    }
    let import_stdout = String::from_utf8_lossy(&import.stdout);
    let import_stderr = String::from_utf8_lossy(&import.stderr);
    if import_stdout.contains("abandon") || import_stderr.contains("abandon") {
        return Err(format!(
            "wallet import leaked seed words: stdout={import_stdout}, stderr={import_stderr}"
        ));
    }

    let addr0_str = run_wallet_ok(config, dir.path(), &["wallet", "address"], None)?;
    if addr0_str != REGTEST_EXTERNAL_0 {
        return Err(format!(
            "expected first address `{REGTEST_EXTERNAL_0}`, got `{addr0_str}`"
        ));
    }
    if addr0_str.contains("abandon") {
        return Err(format!("address output leaked seed word: {addr0_str}"));
    }

    let addr1_str = run_wallet_ok(config, dir.path(), &["wallet", "address"], None)?;
    if addr1_str != REGTEST_EXTERNAL_1 {
        return Err(format!(
            "expected second address `{REGTEST_EXTERNAL_1}`, got `{addr1_str}`"
        ));
    }

    let addr2_str = run_wallet_ok(config, dir.path(), &["wallet", "address"], None)?;
    if !addr2_str.starts_with("bcrt1q") || addr2_str == addr0_str || addr2_str == addr1_str {
        return Err(format!(
            "expected distinct bcrt1q third address, got `{addr2_str}`"
        ));
    }
    Ok(())
}

fn import_via_stdin_and_address(config: &E2eConfig) -> Result<(), String> {
    let dir = TempWalletDir::new("import_stdin")?;
    run_wallet_ok(
        config,
        dir.path(),
        &["wallet", "import"],
        Some(&format!("{BIP39_VECTOR}\n")),
    )?;

    let actual = run_wallet_ok(config, dir.path(), &["wallet", "address"], None)?;
    if actual != REGTEST_EXTERNAL_0 {
        return Err(format!(
            "expected `{REGTEST_EXTERNAL_0}` after stdin import, got `{actual}`"
        ));
    }
    Ok(())
}

fn network_env_overrides_and_invalid_network(config: &E2eConfig) -> Result<(), String> {
    let mainnet_dir = TempWalletDir::new("genesis_mismatch")?;
    let mismatch = run_wallet(
        config,
        mainnet_dir.path(),
        Some("mainnet"),
        &["wallet", "create"],
        None,
    )?;
    if mismatch.status.success() {
        return Err(
            "wallet create with DNS_NOSTR_NETWORK=mainnet against regtest node must exit non-zero"
                .to_string(),
        );
    }
    let stderr = String::from_utf8_lossy(&mismatch.stderr);
    let mainnet_genesis =
        bitcoin::constants::genesis_block(Network::Bitcoin).block_hash().to_string();
    let regtest_genesis =
        bitcoin::constants::genesis_block(Network::Regtest).block_hash().to_string();
    if !stderr.contains(&mainnet_genesis) || !stderr.contains(&regtest_genesis) {
        return Err(format!(
            "expected expected/actual genesis hashes in stderr, got: {stderr}"
        ));
    }
    if mainnet_dir.path().join("wallet.sqlite").exists() {
        return Err(
            "failed genesis-mismatch wallet create must not create wallet.sqlite".to_string(),
        );
    }

    let invalid_dir = TempWalletDir::new("invalid_network")?;
    let invalid = run_wallet(
        config,
        invalid_dir.path(),
        Some("dogecoin"),
        &["wallet", "create"],
        None,
    )?;
    if invalid.status.success() {
        return Err("invalid DNS_NOSTR_NETWORK should exit non-zero".to_string());
    }
    Ok(())
}

fn error_paths_do_not_leak_seed_words(config: &E2eConfig) -> Result<(), String> {
    let dir = TempWalletDir::new("errors")?;

    let missing_wallet = run_wallet(config, dir.path(), None, &["wallet", "address"], None)?;
    if missing_wallet.status.success() {
        return Err("wallet address without an initialized wallet must exit non-zero".to_string());
    }

    let bad_import = run_wallet(
        config,
        dir.path(),
        None,
        &["wallet", "import", "zebra", "zebra"],
        None,
    )?;
    if bad_import.status.success() {
        return Err("invalid mnemonic import must exit non-zero".to_string());
    }
    let bad_stdout = String::from_utf8_lossy(&bad_import.stdout);
    let bad_stderr = String::from_utf8_lossy(&bad_import.stderr);
    if bad_stdout.contains("zebra") {
        return Err(format!("stdout leaked seed word: {bad_stdout}"));
    }
    if bad_stderr.contains("zebra") {
        return Err(format!("stderr leaked seed word: {bad_stderr}"));
    }

    let unknown_flag = run_wallet(
        config,
        dir.path(),
        None,
        &["wallet", "address", "--index", "0"],
        None,
    )?;
    if unknown_flag.status.success() {
        return Err("unknown flag `--index 0` must exit non-zero".to_string());
    }
    Ok(())
}

fn receive_and_balance_tracking(
    config: &E2eConfig,
    bitcoin_client: &RegtestTokenClient,
) -> Result<(), String> {
    let dir = TempWalletDir::new("receive_balance")?;
    run_wallet_ok(config, dir.path(), &["wallet", "create"], None)?;
    let addr_str = run_wallet_ok(config, dir.path(), &["wallet", "address"], None)?;
    let addr = parse_regtest_address(&addr_str)?;

    bitcoin_client.fund_address_unconfirmed(&addr, Amount::from_btc(1.5).unwrap())?;

    let unconfirmed_balance = run_wallet_ok(config, dir.path(), &["wallet", "balance"], None)?;
    if unconfirmed_balance != "0" {
        return Err(format!(
            "expected pre-confirmation balance `0`, got `{unconfirmed_balance}`"
        ));
    }

    bitcoin_client.mine_one_block()?;

    let confirmed_balance = run_wallet_ok(config, dir.path(), &["wallet", "balance"], None)?;
    if confirmed_balance != "1.5" {
        return Err(format!(
            "expected confirmed balance `1.5`, got `{confirmed_balance}`"
        ));
    }
    Ok(())
}

fn send_and_consecutive_unconfirmed_change_spend(
    config: &E2eConfig,
    bitcoin_client: &RegtestTokenClient,
) -> Result<(), String> {
    let sender_dir = TempWalletDir::new("send_sender")?;
    let recipient_dir = TempWalletDir::new("send_recipient")?;

    run_wallet_ok(config, sender_dir.path(), &["wallet", "create"], None)?;
    run_wallet_ok(config, recipient_dir.path(), &["wallet", "create"], None)?;

    let sender_addr = parse_regtest_address(&run_wallet_ok(
        config,
        sender_dir.path(),
        &["wallet", "address"],
        None,
    )?)?;
    bitcoin_client.fund_address(&sender_addr, Amount::from_btc(1.0).unwrap())?;

    let recipient_addr_1 = run_wallet_ok(config, recipient_dir.path(), &["wallet", "address"], None)?;
    let recipient_addr_2 = run_wallet_ok(config, recipient_dir.path(), &["wallet", "address"], None)?;

    let txid1 = run_wallet_ok(
        config,
        sender_dir.path(),
        &["wallet", "send", &recipient_addr_1, "0.25"],
        None,
    )?;
    if txid1.len() != 64 || !txid1.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("expected bare 64-hex txid, got `{txid1}`"));
    }

    let balance_after_first_str =
        run_wallet_ok(config, sender_dir.path(), &["wallet", "balance"], None)?;
    let balance_after_first = parse_btc_amount(&balance_after_first_str)?;
    if balance_after_first >= Amount::from_btc(0.75).unwrap()
        || balance_after_first <= Amount::from_btc(0.70).unwrap()
    {
        return Err(format!(
            "expected trusted_pending balance below 0.75 BTC after first send, got `{balance_after_first_str}`"
        ));
    }

    let txid2 = run_wallet_ok(
        config,
        sender_dir.path(),
        &["wallet", "send", &recipient_addr_2, "0.1"],
        None,
    )?;
    if txid2.len() != 64 || !txid2.chars().all(|c| c.is_ascii_hexdigit()) || txid2 == txid1 {
        return Err(format!("expected distinct 64-hex second txid, got `{txid2}`"));
    }

    let balance_after_second_str =
        run_wallet_ok(config, sender_dir.path(), &["wallet", "balance"], None)?;

    bitcoin_client.mine_one_block()?;

    let recipient_balance =
        run_wallet_ok(config, recipient_dir.path(), &["wallet", "balance"], None)?;
    if recipient_balance != "0.35" {
        return Err(format!(
            "expected recipient confirmed balance `0.35`, got `{recipient_balance}`"
        ));
    }

    let sender_confirmed_balance =
        run_wallet_ok(config, sender_dir.path(), &["wallet", "balance"], None)?;
    if sender_confirmed_balance != balance_after_second_str {
        return Err(format!(
            "expected sender confirmed balance `{balance_after_second_str}`, got `{sender_confirmed_balance}`"
        ));
    }
    Ok(())
}

fn historical_import_recovery(
    config: &E2eConfig,
    bitcoin_client: &RegtestTokenClient,
) -> Result<(), String> {
    let original_dir = TempWalletDir::new("import_recovery_orig")?;
    let create_stdout = run_wallet_ok(config, original_dir.path(), &["wallet", "create"], None)?;
    let mnemonic = create_stdout
        .lines()
        .last()
        .ok_or_else(|| format!("missing seed phrase line: {create_stdout}"))?
        .trim()
        .to_string();

    let addr = parse_regtest_address(&run_wallet_ok(
        config,
        original_dir.path(),
        &["wallet", "address"],
        None,
    )?)?;
    bitcoin_client.fund_address(&addr, Amount::from_btc(0.5).unwrap())?;

    let recovered_dir = TempWalletDir::new("import_recovery_restored")?;
    run_wallet_ok(
        config,
        recovered_dir.path(),
        &["wallet", "import"],
        Some(&format!("{mnemonic}\n")),
    )?;

    let recovered_balance =
        run_wallet_ok(config, recovered_dir.path(), &["wallet", "balance"], None)?;
    if recovered_balance != "0.5" {
        return Err(format!(
            "expected recovered balance `0.5`, got `{recovered_balance}`"
        ));
    }
    Ok(())
}
