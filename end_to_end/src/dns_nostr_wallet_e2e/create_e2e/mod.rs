mod generates_seed_and_sets_0600_permissions;
mod rejects_duplicate_wallet;
mod rejects_genesis_mismatch_network;
mod rejects_invalid_network;

use super::utils::wallet_cli::WalletCli;
use crate::utils::scenario_runner::{report_step, ScenarioError};

pub fn run_all(wallet_cli: &WalletCli<'_>) -> Result<usize, ScenarioError> {
    let mut passed = 0;
    passed += report_step(
        "dns_nostr_wallet::create::generates_seed_and_sets_0600_permissions",
        generates_seed_and_sets_0600_permissions::run(wallet_cli),
    )?;
    passed += report_step(
        "dns_nostr_wallet::create::rejects_duplicate_wallet",
        rejects_duplicate_wallet::run(wallet_cli),
    )?;
    passed += report_step(
        "dns_nostr_wallet::create::rejects_genesis_mismatch_network",
        rejects_genesis_mismatch_network::run(wallet_cli),
    )?;
    passed += report_step(
        "dns_nostr_wallet::create::rejects_invalid_network",
        rejects_invalid_network::run(wallet_cli),
    )?;
    Ok(passed)
}
