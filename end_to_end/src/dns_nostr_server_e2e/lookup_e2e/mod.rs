mod not_found_returns_nxdomain;

use super::utils::dns_client::DnsClient;
use crate::utils::{
    config::E2eConfig,
    scenario_runner::{report_step, ScenarioError},
};

pub async fn run_all(config: &E2eConfig, dns_client: &DnsClient) -> Result<usize, ScenarioError> {
    let mut passed = 0;
    passed += report_step(
        "dns_nostr_server::lookup::not_found_returns_nxdomain",
        not_found_returns_nxdomain::run(config, dns_client).await,
    )?;
    Ok(passed)
}
