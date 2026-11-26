use dns_nostr_server::{
    blockchain_watcher::{BlockchainWatcher, BlockchainWatcherConfig, WatcherFatalError},
    environment_config::EnvironmentConfig,
    nostr_authority::NostrAuthority,
    nostr_events_repository::NostrEventsRepository,
    sqlite_name_token_repository::SqliteNameTokenRepository,
};
use dns_nostr_token::dns_nostr_service::DnsNostrService;
use hickory_server::{
    authority::{Authority, Catalog},
    ServerFuture,
};
use name_token::{NameTokenService, NameTokenServiceError};
use std::{process::ExitCode, sync::Arc};
use tokio::net::UdpSocket;

#[tokio::main]
async fn main() -> ExitCode {
    let config = match EnvironmentConfig::from_environment() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("Configuration error: {error}");
            return ExitCode::FAILURE;
        }
    };
    println!("Starting dns_nostr_server with config: {config:?}");

    let name_token_repository = match SqliteNameTokenRepository::open(&config.db_path) {
        Ok(repository) => Box::new(repository),
        Err(error) => {
            eprintln!(
                "Failed to open SQLite database at `{}`: {error}",
                config.db_path.display()
            );
            return ExitCode::FAILURE;
        }
    };

    let name_token_service = match NameTokenService::new(name_token_repository).await {
        Ok(service) => Arc::new(service),
        Err(NameTokenServiceError::MissingGenesis) => {
            eprintln!(
                "Error: persisted SQLite database `{}` is missing height 0 (genesis). \
                 Remove it to resync from scratch: rm {}*",
                config.db_path.display(),
                config.db_path.display()
            );
            return ExitCode::FAILURE;
        }
    };

    let watcher_config = BlockchainWatcherConfig {
        rpc_url: config.bitcoin_rpc_url,
        rpc_user: config.bitcoin_rpc_user,
        rpc_password: config.bitcoin_rpc_password,
        sync_interval: config.sync_interval,
    };
    let watcher_handle = BlockchainWatcher::new(name_token_service.clone(), watcher_config).spawn();
    let dns_nostr_token_service = DnsNostrService::new(name_token_service);
    let nostr_events_repository = NostrEventsRepository::new(config.nostr_relay_url);

    let mut handler = Catalog::new();
    let nostr_authority = NostrAuthority::new(
        config.zone_origin,
        dns_nostr_token_service,
        nostr_events_repository,
    );
    handler.upsert(
        nostr_authority.origin().clone(),
        Box::new(Arc::new(nostr_authority)),
    );

    let udp_socket = match UdpSocket::bind(config.bind_addr).await {
        Ok(socket) => socket,
        Err(error) => {
            eprintln!("Failed to bind UDP socket on `{}`: {error}", config.bind_addr);
            return ExitCode::FAILURE;
        }
    };

    let mut server = ServerFuture::new(handler);
    server.register_socket(udp_socket);

    tokio::select! {
        watcher_result = watcher_handle => {
            match watcher_result {
                Ok(Err(WatcherFatalError::GenesisMismatch { stored, node })) => {
                    eprintln!(
                        "Fatal blockchain watcher error: SQLite genesis ({stored}) does not match Bitcoin node genesis ({node}). \
                         Remove the SQLite database to resync: rm {}*",
                        config.db_path.display()
                    );
                }
                Ok(Ok(())) => {
                    eprintln!("Fatal error: blockchain watcher exited unexpectedly.");
                }
                Err(join_error) => {
                    eprintln!("Fatal error: blockchain watcher task terminated unexpectedly: {join_error}");
                }
            }
            ExitCode::FAILURE
        }
        server_result = server.block_until_done() => {
            match server_result {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("DNS server exited with error: {error}");
                    ExitCode::FAILURE
                }
            }
        }
    }
}
