use super::super::utils::{temp_wallet_dir::TempWalletDir, wallet_cli::WalletCli};

pub fn run(wallet_cli: &WalletCli<'_>) -> Result<(), String> {
    let invalid_dir = TempWalletDir::new("invalid_network")?;
    let invalid = wallet_cli.run(
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
