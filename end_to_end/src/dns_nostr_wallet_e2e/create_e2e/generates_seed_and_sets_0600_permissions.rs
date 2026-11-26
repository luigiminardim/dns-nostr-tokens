use super::super::utils::{temp_wallet_dir::TempWalletDir, wallet_cli::WalletCli};

pub fn run(wallet_cli: &WalletCli<'_>) -> Result<(), String> {
    let dir = TempWalletDir::new("create")?;
    let output = wallet_cli.run(dir.path(), None, &["wallet", "create"], None)?;
    if !output.status.success() {
        return Err(format!(
            "wallet create failed: stderr={}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
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

    Ok(())
}
