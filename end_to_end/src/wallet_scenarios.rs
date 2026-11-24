use crate::config::E2eConfig;
use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
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
        std::fs::create_dir_all(&dir)
            .map_err(|error| format!("failed to create temp wallet dir `{}`: {error}", dir.display()))?;
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

pub fn run_all() -> Result<(), (&'static str, String)> {
    run_step(
        "wallet::create_generates_seed_sets_0600_and_rejects_duplicate",
        create_generates_seed_sets_0600_and_rejects_duplicate(),
    )?;
    run_step(
        "wallet::import_args_and_address_auto_advance",
        import_args_and_address_auto_advance(),
    )?;
    run_step(
        "wallet::import_via_stdin_and_address",
        import_via_stdin_and_address(),
    )?;
    run_step(
        "wallet::network_env_overrides_and_invalid_network",
        network_env_overrides_and_invalid_network(),
    )?;
    run_step(
        "wallet::error_paths_do_not_leak_seed_words",
        error_paths_do_not_leak_seed_words(),
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

fn create_generates_seed_sets_0600_and_rejects_duplicate() -> Result<(), String> {
    let dir = TempWalletDir::new("create")?;
    let first = run_wallet(dir.path(), None, &["wallet", "create"], None)?;
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

    let second = run_wallet(dir.path(), None, &["wallet", "create"], None)?;
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

fn import_args_and_address_auto_advance() -> Result<(), String> {
    let dir = TempWalletDir::new("import_args")?;
    let mut import_args = vec!["wallet", "import"];
    import_args.extend(BIP39_VECTOR.split_whitespace());

    let import = run_wallet(dir.path(), None, &import_args, None)?;
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

    let addr0 = run_wallet(dir.path(), None, &["wallet", "address"], None)?;
    if !addr0.status.success() {
        return Err(format!(
            "wallet address #0 failed: stderr={}",
            String::from_utf8_lossy(&addr0.stderr)
        ));
    }
    let addr0_str = String::from_utf8_lossy(&addr0.stdout).trim().to_string();
    if addr0_str != REGTEST_EXTERNAL_0 {
        return Err(format!(
            "expected first address `{REGTEST_EXTERNAL_0}`, got `{addr0_str}`"
        ));
    }
    if addr0_str.contains("abandon") {
        return Err(format!("address output leaked seed word: {addr0_str}"));
    }

    let addr1 = run_wallet(dir.path(), None, &["wallet", "address"], None)?;
    if !addr1.status.success() {
        return Err(format!(
            "wallet address #1 failed: stderr={}",
            String::from_utf8_lossy(&addr1.stderr)
        ));
    }
    let addr1_str = String::from_utf8_lossy(&addr1.stdout).trim().to_string();
    if addr1_str != REGTEST_EXTERNAL_1 {
        return Err(format!(
            "expected second address `{REGTEST_EXTERNAL_1}`, got `{addr1_str}`"
        ));
    }

    let addr2 = run_wallet(dir.path(), None, &["wallet", "address"], None)?;
    if !addr2.status.success() {
        return Err(format!(
            "wallet address #2 failed: stderr={}",
            String::from_utf8_lossy(&addr2.stderr)
        ));
    }
    let addr2_str = String::from_utf8_lossy(&addr2.stdout).trim().to_string();
    if !addr2_str.starts_with("bcrt1q") || addr2_str == addr0_str || addr2_str == addr1_str {
        return Err(format!(
            "expected distinct bcrt1q third address, got `{addr2_str}`"
        ));
    }
    Ok(())
}

fn import_via_stdin_and_address() -> Result<(), String> {
    let dir = TempWalletDir::new("import_stdin")?;
    let import = run_wallet(
        dir.path(),
        None,
        &["wallet", "import"],
        Some(&format!("{BIP39_VECTOR}\n")),
    )?;
    if !import.status.success() {
        return Err(format!(
            "wallet import via stdin failed: stderr={}",
            String::from_utf8_lossy(&import.stderr)
        ));
    }

    let address = run_wallet(dir.path(), None, &["wallet", "address"], None)?;
    if !address.status.success() {
        return Err(format!(
            "wallet address after stdin import failed: stderr={}",
            String::from_utf8_lossy(&address.stderr)
        ));
    }
    let actual = String::from_utf8_lossy(&address.stdout).trim().to_string();
    if actual != REGTEST_EXTERNAL_0 {
        return Err(format!(
            "expected `{REGTEST_EXTERNAL_0}` after stdin import, got `{actual}`"
        ));
    }
    Ok(())
}

fn network_env_overrides_and_invalid_network() -> Result<(), String> {
    let testnet_dir = TempWalletDir::new("testnet")?;
    let import_testnet = run_wallet(
        testnet_dir.path(),
        Some("testnet"),
        &["wallet", "import"],
        Some(BIP39_VECTOR),
    )?;
    if !import_testnet.status.success() {
        return Err("testnet wallet import failed".to_string());
    }
    let testnet_addr = run_wallet(
        testnet_dir.path(),
        Some("testnet"),
        &["wallet", "address"],
        None,
    )?;
    if !testnet_addr.status.success() {
        return Err("testnet wallet address failed".to_string());
    }
    let testnet_addr_str = String::from_utf8_lossy(&testnet_addr.stdout)
        .trim()
        .to_string();
    if !testnet_addr_str.starts_with("tb1q") {
        return Err(format!(
            "expected tb1q... testnet address, got `{testnet_addr_str}`"
        ));
    }

    let mainnet_dir = TempWalletDir::new("mainnet")?;
    let import_mainnet = run_wallet(
        mainnet_dir.path(),
        Some("mainnet"),
        &["wallet", "import"],
        Some(BIP39_VECTOR),
    )?;
    if !import_mainnet.status.success() {
        return Err("mainnet wallet import failed".to_string());
    }
    let mainnet_addr = run_wallet(
        mainnet_dir.path(),
        Some("mainnet"),
        &["wallet", "address"],
        None,
    )?;
    if !mainnet_addr.status.success() {
        return Err("mainnet wallet address failed".to_string());
    }
    let mainnet_addr_str = String::from_utf8_lossy(&mainnet_addr.stdout)
        .trim()
        .to_string();
    if !mainnet_addr_str.starts_with("bc1q") {
        return Err(format!(
            "expected bc1q... mainnet address, got `{mainnet_addr_str}`"
        ));
    }

    let invalid_dir = TempWalletDir::new("invalid_network")?;
    let invalid = run_wallet(
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

fn error_paths_do_not_leak_seed_words() -> Result<(), String> {
    let dir = TempWalletDir::new("errors")?;

    let missing_wallet = run_wallet(dir.path(), None, &["wallet", "address"], None)?;
    if missing_wallet.status.success() {
        return Err("wallet address without an initialized wallet must exit non-zero".to_string());
    }

    let bad_import = run_wallet(
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
