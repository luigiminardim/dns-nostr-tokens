mod auto_advances_external_addresses;
mod rejects_uninitialized_wallet;
mod rejects_unknown_flag;

use super::utils::wallet_cli::WalletCli;
use crate::utils::scenario_runner::{report_step, ScenarioError};

pub fn run_all(wallet_cli: &WalletCli<'_>) -> Result<usize, ScenarioError> {
    let mut passed = 0;
    passed += report_step(
        "dns_nostr_wallet::address::auto_advances_external_addresses",
        auto_advances_external_addresses::run(wallet_cli),
    )?;
    passed += report_step(
        "dns_nostr_wallet::address::rejects_uninitialized_wallet",
        rejects_uninitialized_wallet::run(wallet_cli),
    )?;
    passed += report_step(
        "dns_nostr_wallet::address::rejects_unknown_flag",
        rejects_unknown_flag::run(wallet_cli),
    )?;
    Ok(passed)
}
