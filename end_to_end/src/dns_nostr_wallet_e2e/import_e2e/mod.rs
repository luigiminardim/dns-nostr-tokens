mod import_from_cli_args;
mod import_from_stdin;
mod recovers_historical_balance;
mod rejects_invalid_mnemonic_without_leaking_seed;

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
        "dns_nostr_wallet::import::import_from_cli_args",
        import_from_cli_args::run(wallet_cli),
    )?;
    passed += report_step(
        "dns_nostr_wallet::import::import_from_stdin",
        import_from_stdin::run(wallet_cli),
    )?;
    passed += report_step(
        "dns_nostr_wallet::import::rejects_invalid_mnemonic_without_leaking_seed",
        rejects_invalid_mnemonic_without_leaking_seed::run(wallet_cli),
    )?;
    passed += report_step(
        "dns_nostr_wallet::import::recovers_historical_balance",
        recovers_historical_balance::run(wallet_cli, bitcoin_client),
    )?;
    Ok(passed)
}
