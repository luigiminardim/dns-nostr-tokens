mod updated_rotates_to_new_nostr_pubkey;

use super::utils::{dns_client::DnsClient, token_tx::TokenTxClient};
use crate::utils::{
    config::E2eConfig,
    scenario_runner::{report_step, ScenarioError},
};

pub async fn run_all(
    config: &E2eConfig,
    dns_client: &DnsClient,
    token_client: &TokenTxClient<'_>,
) -> Result<usize, ScenarioError> {
    let mut passed = 0;
    passed += report_step(
        "dns_nostr_server::update::updated_rotates_to_new_nostr_pubkey",
        updated_rotates_to_new_nostr_pubkey::run(config, dns_client, token_client).await,
    )?;
    Ok(passed)
}
