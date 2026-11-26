use super::super::utils::{temp_wallet_dir::TempWalletDir, wallet_cli::WalletCli};
use crate::utils::regtest_bitcoin::RegtestBitcoinClient;
use bitcoin::Amount;

pub fn run(
    wallet_cli: &WalletCli<'_>,
    bitcoin_client: &RegtestBitcoinClient,
) -> Result<(), String> {
    let original_dir = TempWalletDir::new("import_recovery_orig")?;
    let create_stdout = wallet_cli.run_ok(original_dir.path(), &["wallet", "create"], None)?;
    let mnemonic = create_stdout
        .lines()
        .last()
        .ok_or_else(|| format!("missing seed phrase line: {create_stdout}"))?
        .trim()
        .to_string();

    let addr = RegtestBitcoinClient::parse_regtest_address(&wallet_cli.run_ok(
        original_dir.path(),
        &["wallet", "address"],
        None,
    )?)?;
    bitcoin_client.fund_address(&addr, Amount::from_btc(0.5).unwrap())?;

    let recovered_dir = TempWalletDir::new("import_recovery_restored")?;
    wallet_cli.run_ok(
        recovered_dir.path(),
        &["wallet", "import"],
        Some(&format!("{mnemonic}\n")),
    )?;

    let recovered_balance =
        wallet_cli.run_ok(recovered_dir.path(), &["wallet", "balance"], None)?;
    if recovered_balance != "0.5" {
        return Err(format!(
            "expected recovered balance `0.5`, got `{recovered_balance}`"
        ));
    }
    Ok(())
}
