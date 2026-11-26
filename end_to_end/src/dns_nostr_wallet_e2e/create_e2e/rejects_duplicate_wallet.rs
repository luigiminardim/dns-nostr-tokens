use super::super::utils::{temp_wallet_dir::TempWalletDir, wallet_cli::WalletCli};

pub fn run(wallet_cli: &WalletCli<'_>) -> Result<(), String> {
    let dir = TempWalletDir::new("create_duplicate")?;
    wallet_cli.run_ok(dir.path(), &["wallet", "create"], None)?;

    let db_path = dir.path().join("wallet.sqlite");
    let db_bytes_before = std::fs::read(&db_path)
        .map_err(|error| format!("failed to read wallet.sqlite: {error}"))?;

    let second = wallet_cli.run(dir.path(), None, &["wallet", "create"], None)?;
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
