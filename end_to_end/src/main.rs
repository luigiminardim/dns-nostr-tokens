mod dns_nostr_server_e2e;
mod dns_nostr_wallet_e2e;
mod utils;

use std::process::ExitCode;
use utils::{config::E2eConfig, regtest_bitcoin::RegtestBitcoinClient};

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

    let bitcoin_client = match RegtestBitcoinClient::new(
        &config.bitcoin_rpc_user,
        &config.bitcoin_rpc_password,
    ) {
        Ok(client) => client,
        Err(error) => {
            eprintln!("[FAIL] bitcoin rpc initialization: {error}");
            return ExitCode::FAILURE;
        }
    };

    let mut passed = 0;

    match dns_nostr_server_e2e::run_all(&config, &bitcoin_client).await {
        Ok(count) => passed += count,
        Err((scenario, error)) => {
            eprintln!("[FAIL] {scenario}: {error}");
            return ExitCode::FAILURE;
        }
    }

    match dns_nostr_wallet_e2e::run_all(&config, &bitcoin_client) {
        Ok(count) => passed += count,
        Err((scenario, error)) => {
            eprintln!("[FAIL] {scenario}: {error}");
            return ExitCode::FAILURE;
        }
    }

    println!("All {passed} end-to-end scenarios passed.");
    ExitCode::SUCCESS
}
