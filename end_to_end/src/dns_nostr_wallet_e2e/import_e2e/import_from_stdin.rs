use super::super::utils::{
    temp_wallet_dir::TempWalletDir,
    wallet_cli::{WalletCli, BIP39_VECTOR, REGTEST_EXTERNAL_0},
};

pub fn run(wallet_cli: &WalletCli<'_>) -> Result<(), String> {
    let dir = TempWalletDir::new("import_stdin")?;
    wallet_cli.run_ok(
        dir.path(),
        &["wallet", "import"],
        Some(&format!("{BIP39_VECTOR}\n")),
    )?;

    let actual = wallet_cli.run_ok(dir.path(), &["wallet", "address"], None)?;
    if actual != REGTEST_EXTERNAL_0 {
        return Err(format!(
            "expected `{REGTEST_EXTERNAL_0}` after stdin import, got `{actual}`"
        ));
    }
    Ok(())
}
