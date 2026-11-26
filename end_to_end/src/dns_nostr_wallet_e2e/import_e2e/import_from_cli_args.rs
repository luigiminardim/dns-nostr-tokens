use super::super::utils::{
    temp_wallet_dir::TempWalletDir,
    wallet_cli::{WalletCli, BIP39_VECTOR, REGTEST_EXTERNAL_0},
};

pub fn run(wallet_cli: &WalletCli<'_>) -> Result<(), String> {
    let dir = TempWalletDir::new("import_args")?;
    let mut import_args = vec!["wallet", "import"];
    import_args.extend(BIP39_VECTOR.split_whitespace());

    let import = wallet_cli.run(dir.path(), None, &import_args, None)?;
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

    let addr0_str = wallet_cli.run_ok(dir.path(), &["wallet", "address"], None)?;
    if addr0_str != REGTEST_EXTERNAL_0 {
        return Err(format!(
            "expected first address `{REGTEST_EXTERNAL_0}`, got `{addr0_str}`"
        ));
    }
    if addr0_str.contains("abandon") {
        return Err(format!("address output leaked seed word: {addr0_str}"));
    }
    Ok(())
}
