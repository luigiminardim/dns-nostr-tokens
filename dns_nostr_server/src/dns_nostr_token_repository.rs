use std::{future::Future, sync::Arc};

use crate::{
    dns_nostr_token::DnsNostrToken, name_token::Bytes, name_token_repository::NameTokenRepository,
};
use hickory_server::proto::rr::domain::Label;

pub trait GetDnsNostrToken: Send + Sync {
    fn get_token(&self, label: &Label) -> impl Future<Output = Option<DnsNostrToken>> + Send;
}

pub struct DnsNostrTokenRepository {
    name_token_repository: Arc<NameTokenRepository>,
}

impl DnsNostrTokenRepository {
    pub fn new(name_token_repository: Arc<NameTokenRepository>) -> Self {
        DnsNostrTokenRepository {
            name_token_repository,
        }
    }

    pub async fn get_token(&self, label: &Label) -> Option<DnsNostrToken> {
        let label = Bytes::from(label.as_bytes());
        let name_token = self.name_token_repository.get_name_token(&label).await;
        let dns_nostr_token = match name_token {
            None => return None,
            Some(name_token) => DnsNostrToken::try_from(name_token),
        };
        match dns_nostr_token {
            Err(_) => None,
            Ok(dns_nostr_token) => Some(dns_nostr_token),
        }
    }
}

impl GetDnsNostrToken for DnsNostrTokenRepository {
    async fn get_token(&self, label: &Label) -> Option<DnsNostrToken> {
        self.get_token(label).await
    }
}
