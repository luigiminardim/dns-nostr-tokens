use super::super::utils::{
    temp_wallet_dir::TempWalletDir,
    wallet_cli::{WalletCli, BIP39_VECTOR, REGTEST_EXTERNAL_0},
};

pub fn run(wallet_cli: &WalletCli<'_>) -> Result<(), String> {
    let regtest_external_1 = "bcrt1qd7spv5q28348xl4myc8zmh983w5jx32cs707jh";
    let dir = TempWalletDir::new("address_advance")?;
    wallet_cli.run_ok(
        dir.path(),
        &["wallet", "import"],
        Some(&format!("{BIP39_VECTOR}\n")),
    )?;

    let addr0_str = wallet_cli.run_ok(dir.path(), &["wallet", "address"], None)?;
    if addr0_str != REGTEST_EXTERNAL_0 {
        return Err(format!(
            "expected first address `{REGTEST_EXTERNAL_0}`, got `{addr0_str}`"
        ));
    }
    if addr0_str.contains("abandon") {
        return Err(format!("address output leaked seed word: {addr0_str}"));
    }

    let addr1_str = wallet_cli.run_ok(dir.path(), &["wallet", "address"], None)?;
    if addr1_str != regtest_external_1 {
        return Err(format!(
            "expected second address `{regtest_external_1}`, got `{addr1_str}`"
        ));
    }

    let addr2_str = wallet_cli.run_ok(dir.path(), &["wallet", "address"], None)?;
    if !addr2_str.starts_with("bcrt1q") || addr2_str == addr0_str || addr2_str == addr1_str {
        return Err(format!(
            "expected distinct bcrt1q third address, got `{addr2_str}`"
        ));
    }
    Ok(())
}
