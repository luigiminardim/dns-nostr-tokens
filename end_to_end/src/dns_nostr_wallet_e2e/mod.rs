mod address_e2e;
mod balance_e2e;
mod create_e2e;
mod import_e2e;
mod send_e2e;
mod utils;

use crate::utils::{
    config::E2eConfig, regtest_bitcoin::RegtestBitcoinClient, scenario_runner::ScenarioError,
};
use utils::wallet_cli::WalletCli;

pub fn run_all(
    config: &E2eConfig,
    bitcoin_client: &RegtestBitcoinClient,
) -> Result<usize, ScenarioError> {
    let wallet_cli = WalletCli::new(config);
    let mut passed = 0;
    passed += create_e2e::run_all(&wallet_cli)?;
    passed += import_e2e::run_all(&wallet_cli, bitcoin_client)?;
    passed += address_e2e::run_all(&wallet_cli)?;
    passed += balance_e2e::run_all(&wallet_cli, bitcoin_client)?;
    passed += send_e2e::run_all(&wallet_cli, bitcoin_client)?;
    Ok(passed)
}
