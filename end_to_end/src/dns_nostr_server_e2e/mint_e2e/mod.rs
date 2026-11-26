mod first_is_root_ignores_competing_mint;
mod minted_resolves_txt_pubkey;

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
        "dns_nostr_server::mint::minted_resolves_txt_pubkey",
        minted_resolves_txt_pubkey::run(config, dns_client, token_client).await,
    )?;
    passed += report_step(
        "dns_nostr_server::mint::first_is_root_ignores_competing_mint",
        first_is_root_ignores_competing_mint::run(config, dns_client, token_client).await,
    )?;
    Ok(passed)
}
