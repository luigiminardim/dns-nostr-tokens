use std::{future::Future, sync::Arc};

use crate::dns_nostr_token::DnsNostrToken;
use hickory_server::proto::rr::domain::Label;
use name_token::{Bytes, NameTokenRepository, NameTokenService};

pub trait GetDnsNostrToken: Send + Sync {
    fn get_token(&self, label: &Label) -> impl Future<Output = Option<DnsNostrToken>> + Send;
}

pub struct DnsNostrTokenRepository<R: NameTokenRepository> {
    name_token_service: Arc<NameTokenService<R>>,
}

impl<R: NameTokenRepository> DnsNostrTokenRepository<R> {
    pub fn new(name_token_service: Arc<NameTokenService<R>>) -> Self {
        DnsNostrTokenRepository { name_token_service }
    }

    pub async fn get_token(&self, label: &Label) -> Option<DnsNostrToken> {
        let label = Bytes::from(label.as_bytes());
        let name_token = self.name_token_service.get_name_token(&label).await;
        let dns_nostr_token = match name_token {
            None => return None,
            Some(name_token) => DnsNostrToken::try_from(name_token),
        };
        dns_nostr_token.ok()
    }
}

impl<R: NameTokenRepository> GetDnsNostrToken for DnsNostrTokenRepository<R> {
    async fn get_token(&self, label: &Label) -> Option<DnsNostrToken> {
        self.get_token(label).await
    }
}
