mod tracks_unconfirmed_and_confirmed_receive;

use super::utils::wallet_cli::WalletCli;
use crate::utils::{
    regtest_bitcoin::RegtestBitcoinClient,
    scenario_runner::{report_step, ScenarioError},
};

pub fn run_all(
    wallet_cli: &WalletCli<'_>,
    bitcoin_client: &RegtestBitcoinClient,
) -> Result<usize, ScenarioError> {
    let mut passed = 0;
    passed += report_step(
        "dns_nostr_wallet::balance::tracks_unconfirmed_and_confirmed_receive",
        tracks_unconfirmed_and_confirmed_receive::run(wallet_cli, bitcoin_client),
    )?;
    Ok(passed)
}
