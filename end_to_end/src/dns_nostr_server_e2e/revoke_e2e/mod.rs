mod revoked_returns_nxdomain;

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
        "dns_nostr_server::revoke::revoked_returns_nxdomain",
        revoked_returns_nxdomain::run(config, dns_client, token_client).await,
    )?;
    Ok(passed)
}
