use super::super::utils::{temp_wallet_dir::TempWalletDir, wallet_cli::WalletCli};

pub fn run(wallet_cli: &WalletCli<'_>) -> Result<(), String> {
    let dir = TempWalletDir::new("address_uninitialized")?;
    let missing_wallet = wallet_cli.run(dir.path(), None, &["wallet", "address"], None)?;
    if missing_wallet.status.success() {
        return Err(
            "wallet address without an initialized wallet must exit non-zero".to_string(),
        );
    }
    Ok(())
}
