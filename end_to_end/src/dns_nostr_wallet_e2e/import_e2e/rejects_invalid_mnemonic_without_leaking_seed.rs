use super::super::utils::{temp_wallet_dir::TempWalletDir, wallet_cli::WalletCli};

pub fn run(wallet_cli: &WalletCli<'_>) -> Result<(), String> {
    let dir = TempWalletDir::new("import_invalid_mnemonic")?;
    let bad_import = wallet_cli.run(
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
    Ok(())
}
