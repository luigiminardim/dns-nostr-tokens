use super::super::utils::{temp_wallet_dir::TempWalletDir, wallet_cli::WalletCli};
use crate::utils::regtest_bitcoin::RegtestBitcoinClient;
use bitcoin::Amount;

pub fn run(
    wallet_cli: &WalletCli<'_>,
    bitcoin_client: &RegtestBitcoinClient,
) -> Result<(), String> {
    let dir = TempWalletDir::new("receive_balance")?;
    wallet_cli.run_ok(dir.path(), &["wallet", "create"], None)?;
    let addr_str = wallet_cli.run_ok(dir.path(), &["wallet", "address"], None)?;
    let addr = RegtestBitcoinClient::parse_regtest_address(&addr_str)?;

    bitcoin_client.fund_address_unconfirmed(&addr, Amount::from_btc(1.5).unwrap())?;

    let unconfirmed_balance = wallet_cli.run_ok(dir.path(), &["wallet", "balance"], None)?;
    if unconfirmed_balance != "0" {
        return Err(format!(
            "expected pre-confirmation balance `0`, got `{unconfirmed_balance}`"
        ));
    }

    bitcoin_client.mine_one_block()?;

    let confirmed_balance = wallet_cli.run_ok(dir.path(), &["wallet", "balance"], None)?;
    if confirmed_balance != "1.5" {
        return Err(format!(
            "expected confirmed balance `1.5`, got `{confirmed_balance}`"
        ));
    }
    Ok(())
}
