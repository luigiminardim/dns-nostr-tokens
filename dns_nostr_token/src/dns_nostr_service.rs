use std::sync::Arc;
use name_token::{Bytes, NameTokenService};
use crate::dns_nostr_token::DnsNostrToken;
use crate::label::Label;

pub struct DnsNostrService {
    name_token_service: Arc<NameTokenService>,
}

impl DnsNostrService {
    pub fn new(name_token_service: Arc<NameTokenService>) -> Self {
        Self { name_token_service }
    }

    pub async fn get_token(&self, label: &Label) -> Option<DnsNostrToken> {
        let name_label = Bytes::from(label.as_bytes());
        let name_token = self.name_token_service.get_name_token(&name_label).await?;
        DnsNostrToken::try_from(name_token).ok()
    }
}
