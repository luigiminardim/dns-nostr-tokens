mod bitcoin_token_tx;
mod config;
mod dns_client;
mod labels;
mod nostr_publisher;
mod server_process;
mod server_scenarios;
mod wallet_scenarios;

use bitcoin_token_tx::RegtestTokenClient;
use config::E2eConfig;
use dns_client::DnsClient;
use server_process::ManagedServer;
use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    let config = match E2eConfig::from_environment() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("[FAIL] configuration: {error}");
            return ExitCode::FAILURE;
        }
    };
    println!("Running end_to_end with config: {config:?}");

    let server = match ManagedServer::start(&config).await {
        Ok(server) => server,
        Err(error) => {
            eprintln!("[FAIL] server startup: {error}");
            return ExitCode::FAILURE;
        }
    };

    let dns_client = DnsClient::new(config.dns_server_addr);
    let bitcoin_client = match RegtestTokenClient::new(
        &config.bitcoin_rpc_url,
        &config.bitcoin_rpc_user,
        &config.bitcoin_rpc_password,
    ) {
        Ok(client) => client,
        Err(error) => {
            eprintln!("[FAIL] bitcoin rpc initialization: {error}");
            return ExitCode::FAILURE;
        }
    };

    if let Err((scenario, error)) =
        server_scenarios::run_all(&config, &dns_client, &bitcoin_client).await
    {
        eprintln!("[FAIL] {scenario}: {error}");
        let server_stderr = server.captured_stderr();
        if !server_stderr.trim().is_empty() {
            eprintln!("--- dns_nostr_server stderr ---\n{}", server_stderr.trim());
        }
        return ExitCode::FAILURE;
    }

    drop(server);

    if let Err((scenario, error)) = wallet_scenarios::run_all() {
        eprintln!("[FAIL] {scenario}: {error}");
        return ExitCode::FAILURE;
    }

    println!("All 10 end-to-end scenarios passed.");
    ExitCode::SUCCESS
}
