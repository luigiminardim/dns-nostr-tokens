use super::super::utils::{temp_wallet_dir::TempWalletDir, wallet_cli::WalletCli};
use bitcoin::Network;

pub fn run(wallet_cli: &WalletCli<'_>) -> Result<(), String> {
    let mainnet_dir = TempWalletDir::new("genesis_mismatch")?;
    let mismatch = wallet_cli.run(
        mainnet_dir.path(),
        Some("mainnet"),
        &["wallet", "create"],
        None,
    )?;
    if mismatch.status.success() {
        return Err(
            "wallet create with DNS_NOSTR_NETWORK=mainnet against regtest node must exit non-zero"
                .to_string(),
        );
    }

    let stderr = String::from_utf8_lossy(&mismatch.stderr);
    let mainnet_genesis = bitcoin::constants::genesis_block(Network::Bitcoin)
        .block_hash()
        .to_string();
    let regtest_genesis = bitcoin::constants::genesis_block(Network::Regtest)
        .block_hash()
        .to_string();
    if !stderr.contains(&mainnet_genesis) || !stderr.contains(&regtest_genesis) {
        return Err(format!(
            "expected expected/actual genesis hashes in stderr, got: {stderr}"
        ));
    }
    if mainnet_dir.path().join("wallet.sqlite").exists() {
        return Err(
            "failed genesis-mismatch wallet create must not create wallet.sqlite".to_string(),
        );
    }
    Ok(())
}
