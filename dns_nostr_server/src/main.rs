use hickory_server::{
    authority::{Authority, Catalog},
    ServerFuture,
};
use lib::{
    blockchain_watcher::BlockchainWatcher, dns_nostr_token_repository::DnsNostrTokenRepository,
    nostr_authority::NostrAuthority, nostr_events_repository::NostrEventsRepository,
    sqlite_name_token_repository::SqliteNameTokenRepository,
};
use name_token::NameTokenService;
use std::sync::Arc;
use tokio::net::UdpSocket;

#[tokio::main]
async fn main() {
    let name_token_repository = SqliteNameTokenRepository::create().await;
    let name_token_service = Arc::new(NameTokenService::new(name_token_repository).await);
    BlockchainWatcher::new(name_token_service.clone()).spawn();
    let dns_nostr_token_repository = DnsNostrTokenRepository::new(name_token_service);

    let nostr_events_repository = NostrEventsRepository::new("ws://localhost:8080".to_string());

    let mut handler = Catalog::new();
    let nostr_authority = NostrAuthority::new(
        "nostr.dns.name.".parse().unwrap(),
        dns_nostr_token_repository,
        nostr_events_repository,
    );
    handler.upsert(
        nostr_authority.origin().clone(),
        Box::new(Arc::new(nostr_authority)),
    );
    let mut server = ServerFuture::new(handler);
    server.register_socket(UdpSocket::bind("0.0.0.0:1053").await.unwrap());
    server.block_until_done().await.unwrap();
}
