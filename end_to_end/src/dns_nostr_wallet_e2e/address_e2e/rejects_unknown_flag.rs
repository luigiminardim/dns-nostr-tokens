use super::super::utils::{temp_wallet_dir::TempWalletDir, wallet_cli::WalletCli};

pub fn run(wallet_cli: &WalletCli<'_>) -> Result<(), String> {
    let dir = TempWalletDir::new("address_unknown_flag")?;
    let unknown_flag =
        wallet_cli.run(dir.path(), None, &["wallet", "address", "--index", "0"], None)?;
    if unknown_flag.status.success() {
        return Err("unknown flag `--index 0` must exit non-zero".to_string());
    }
    Ok(())
}
