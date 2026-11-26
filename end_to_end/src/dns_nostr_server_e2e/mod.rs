mod lookup_e2e;
mod mint_e2e;
mod revoke_e2e;
mod utils;
mod update_e2e;

use crate::utils::{
    config::E2eConfig, regtest_bitcoin::RegtestBitcoinClient, scenario_runner::ScenarioError,
};
use utils::{dns_client::DnsClient, server_process::ManagedServer, token_tx::TokenTxClient};

pub async fn run_all(
    config: &E2eConfig,
    bitcoin_client: &RegtestBitcoinClient,
) -> Result<usize, ScenarioError> {
    let server = ManagedServer::start(config)
        .await
        .map_err(|error| ("dns_nostr_server::startup", error))?;

    let dns_client = DnsClient::new(config.dns_server_addr);
    let token_client = TokenTxClient::new(bitcoin_client);

    let result = run_features(config, &dns_client, &token_client).await;
    if result.is_err() {
        let server_stderr = server.captured_stderr();
        if !server_stderr.trim().is_empty() {
            eprintln!("--- dns_nostr_server stderr ---\n{}", server_stderr.trim());
        }
    }
    result
}

async fn run_features(
    config: &E2eConfig,
    dns_client: &DnsClient,
    token_client: &TokenTxClient<'_>,
) -> Result<usize, ScenarioError> {
    let mut passed = 0;
    passed += lookup_e2e::run_all(config, dns_client).await?;
    passed += mint_e2e::run_all(config, dns_client, token_client).await?;
    passed += update_e2e::run_all(config, dns_client, token_client).await?;
    passed += revoke_e2e::run_all(config, dns_client, token_client).await?;
    Ok(passed)
}
