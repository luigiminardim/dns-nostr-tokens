mod sends_and_spends_consecutive_unconfirmed_change;

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
        "dns_nostr_wallet::send::sends_and_spends_consecutive_unconfirmed_change",
        sends_and_spends_consecutive_unconfirmed_change::run(wallet_cli, bitcoin_client),
    )?;
    Ok(passed)
}
