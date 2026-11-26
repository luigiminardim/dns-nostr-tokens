use super::super::utils::{temp_wallet_dir::TempWalletDir, wallet_cli::WalletCli};
use crate::utils::regtest_bitcoin::RegtestBitcoinClient;
use bitcoin::Amount;

pub fn run(
    wallet_cli: &WalletCli<'_>,
    bitcoin_client: &RegtestBitcoinClient,
) -> Result<(), String> {
    let sender_dir = TempWalletDir::new("send_sender")?;
    let recipient_dir = TempWalletDir::new("send_recipient")?;

    wallet_cli.run_ok(sender_dir.path(), &["wallet", "create"], None)?;
    wallet_cli.run_ok(recipient_dir.path(), &["wallet", "create"], None)?;

    let sender_addr = RegtestBitcoinClient::parse_regtest_address(&wallet_cli.run_ok(
        sender_dir.path(),
        &["wallet", "address"],
        None,
    )?)?;
    bitcoin_client.fund_address(&sender_addr, Amount::from_btc(1.0).unwrap())?;

    let recipient_addr_1 = wallet_cli.run_ok(recipient_dir.path(), &["wallet", "address"], None)?;
    let recipient_addr_2 = wallet_cli.run_ok(recipient_dir.path(), &["wallet", "address"], None)?;

    let txid1 = wallet_cli.run_ok(
        sender_dir.path(),
        &["wallet", "send", &recipient_addr_1, "0.25"],
        None,
    )?;
    if txid1.len() != 64 || !txid1.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("expected bare 64-hex txid, got `{txid1}`"));
    }

    let balance_after_first_str =
        wallet_cli.run_ok(sender_dir.path(), &["wallet", "balance"], None)?;
    let balance_after_first = RegtestBitcoinClient::parse_btc_amount(&balance_after_first_str)?;
    if balance_after_first >= Amount::from_btc(0.75).unwrap()
        || balance_after_first <= Amount::from_btc(0.70).unwrap()
    {
        return Err(format!(
            "expected trusted_pending balance below 0.75 BTC after first send, got `{balance_after_first_str}`"
        ));
    }

    let txid2 = wallet_cli.run_ok(
        sender_dir.path(),
        &["wallet", "send", &recipient_addr_2, "0.1"],
        None,
    )?;
    if txid2.len() != 64 || !txid2.chars().all(|c| c.is_ascii_hexdigit()) || txid2 == txid1 {
        return Err(format!(
            "expected distinct 64-hex second txid, got `{txid2}`"
        ));
    }

    let balance_after_second_str =
        wallet_cli.run_ok(sender_dir.path(), &["wallet", "balance"], None)?;

    bitcoin_client.mine_one_block()?;

    let recipient_balance =
        wallet_cli.run_ok(recipient_dir.path(), &["wallet", "balance"], None)?;
    if recipient_balance != "0.35" {
        return Err(format!(
            "expected recipient confirmed balance `0.35`, got `{recipient_balance}`"
        ));
    }

    let sender_confirmed_balance =
        wallet_cli.run_ok(sender_dir.path(), &["wallet", "balance"], None)?;
    if sender_confirmed_balance != balance_after_second_str {
        return Err(format!(
            "expected sender confirmed balance `{balance_after_second_str}`, got `{sender_confirmed_balance}`"
        ));
    }
    Ok(())
}
