use crate::{
    dns_nostr_token_repository::GetDnsNostrToken, nostr_events_repository::NostrEventsRepository,
};
use hickory_server::{
    authority::{Authority, LookupError, LookupOptions, MessageRequest, UpdateResult, ZoneType},
    proto::{
        op::ResponseCode,
        rr::{domain::Label, LowerName, Name, RecordType},
        serialize::txt::Parser,
    },
    server::RequestInfo,
    store::in_memory::InMemoryAuthority,
};

pub struct NostrAuthority<GetTokenT: GetDnsNostrToken> {
    zone: LowerName,
    dns_nostr_token_repository: GetTokenT,
    nostr_events_repository: NostrEventsRepository,
}

#[async_trait::async_trait]
impl<GetTokenT: GetDnsNostrToken> Authority for NostrAuthority<GetTokenT> {
    /// Result of a lookup
    type Lookup = <InMemoryAuthority as Authority>::Lookup;

    fn zone_type(&self) -> ZoneType {
        ZoneType::Primary
    }

    fn is_axfr_allowed(&self) -> bool {
        false
    }

    async fn update(&self, _update: &MessageRequest) -> UpdateResult<bool> {
        Err(ResponseCode::NotImp)
    }

    fn origin(&self) -> &LowerName {
        &self.zone
    }

    async fn lookup(
        &self,
        name: &LowerName,
        rtype: RecordType,
        lookup_options: LookupOptions,
    ) -> Result<Self::Lookup, LookupError> {
        let authority = self
            .create_in_memory_zone_authority(name)
            .await
            .ok_or_else(|| LookupError::from(ResponseCode::NXDomain))?;
        authority.lookup(name, rtype, lookup_options).await
    }

    /// Using the specified query, perform a lookup against this zone.
    ///
    /// # Arguments
    ///
    /// * `query` - the query to perform the lookup with.
    /// * `is_secure` - if true, then RRSIG records (if this is a secure zone) will be returned.
    ///
    /// # Return value
    ///
    /// Returns a vector containing the results of the query, it will be empty if not found. If
    ///  `is_secure` is true, in the case of no records found then NSEC records will be returned.
    async fn search(
        &self,
        request: RequestInfo<'_>,
        lookup_options: LookupOptions,
    ) -> Result<Self::Lookup, LookupError> {
        let authority = self
            .create_in_memory_zone_authority(request.query.name())
            .await
            .ok_or_else(|| LookupError::from(ResponseCode::NXDomain))?;
        authority.search(request, lookup_options).await
    }

    /// Get the NS, NameServer, record for the zone
    async fn ns(&self, lookup_options: LookupOptions) -> Result<Self::Lookup, LookupError> {
        self.lookup(self.origin(), RecordType::NS, lookup_options)
            .await
    }

    /// Return the NSEC records based on the given name
    ///
    /// # Arguments
    ///
    /// * `name` - given this name (i.e. the lookup name), return the NSEC record that is less than
    ///            this
    /// * `is_secure` - if true then it will return RRSIG records as well
    async fn get_nsec_records(
        &self,
        _: &LowerName,
        _: LookupOptions,
    ) -> Result<Self::Lookup, LookupError> {
        Result::Err(LookupError::ResponseCode(ResponseCode::NotImp))
    }

    /// Returns the SOA of the authority.
    ///
    /// *Note*: This will only return the SOA, if this is fulfilling a request, a standard lookup
    ///  should be used, see `soa_secure()`, which will optionally return RRSIGs.
    async fn soa(&self) -> Result<Self::Lookup, LookupError> {
        // SOA should be origin|SOA
        self.lookup(self.origin(), RecordType::SOA, LookupOptions::default())
            .await
    }

    /// Returns the SOA record for the zone
    async fn soa_secure(&self, lookup_options: LookupOptions) -> Result<Self::Lookup, LookupError> {
        self.lookup(self.origin(), RecordType::SOA, lookup_options)
            .await
    }
}

impl<GetTokenT: GetDnsNostrToken> NostrAuthority<GetTokenT> {
    pub fn new(
        zone: LowerName,
        dns_nostr_token_repository: GetTokenT,
        nostr_client: NostrEventsRepository,
    ) -> Self {
        Self {
            zone,
            dns_nostr_token_repository,
            nostr_events_repository: nostr_client,
        }
    }

    async fn create_in_memory_zone_authority(&self, name: &LowerName) -> Option<InMemoryAuthority> {
        let zone_file = self.get_zone_file(name).await?;
        let zone_name = self.extract_zone_name(name)?;
        let (_, records) = Parser::new(&zone_file, None, Some(zone_name.clone()))
            .parse()
            .inspect_err(|e| {
                eprintln!("failed to parse zone file ({:?}):\n: {}", e, &zone_file);
            })
            .ok()?;
        let authority =
            InMemoryAuthority::new(zone_name.clone(), records, ZoneType::Primary, false)
                .inspect_err(|e| {
                    format!(
                        "failed to create authority for {}: {:?}",
                        zone_name.to_string(),
                        e
                    );
                })
                .ok()?;
        Some(authority)
    }

    async fn get_zone_file(&self, name: &LowerName) -> Option<String> {
        let token_label = self.extract_token_label(name)?;
        let dns_nostr_token = self
            .dns_nostr_token_repository
            .get_token(&token_label)
            .await?;
        let last_text_note = self
            .nostr_events_repository
            .get_last_text_note_from_pubkey(dns_nostr_token.nostr_pubkey)
            .await?;
        Some(last_text_note.content)
    }

    /// Check if the domain name has the shape "[<subdomain>.]<label>.<oringin>."
    fn is_valid_dns_nostr_name(&self, name: &LowerName) -> bool {
        name.num_labels() > self.origin().num_labels() && self.origin().zone_of(name)
    }

    /// Extract the label from the queried Name-Token.
    ///
    /// ## Example
    ///
    /// Suppose the origin is nostr.dns.name and the query is "token.nostr.dns.name",
    /// then the label is "token".
    fn extract_token_label(&self, name: &LowerName) -> Option<Label> {
        if !self.is_valid_dns_nostr_name(name) {
            return None;
        }
        let query_name = Name::from(name);
        let raw_label = query_name
            .iter()
            .skip((query_name.num_labels() - self.origin().num_labels() - 1).into())
            .next()?;
        Label::from_raw_bytes(raw_label).ok()
    }

    /// Extract the zone name from the queried Name-Token.
    ///
    /// ## Example
    ///
    /// Suppose the origin is nostr.dns.name and the query is "token.nostr.dns.name",
    /// then the zone name is "nostr.dns.name".
    fn extract_zone_name(&self, name: &LowerName) -> Option<Name> {
        if !self.is_valid_dns_nostr_name(name) {
            return None;
        }
        let zone = Name::from_labels(
            Name::from(name)
                .iter()
                .skip((name.num_labels() - self.origin().num_labels() - 1).into())
                .collect::<Vec<_>>(),
        )
        .ok()?;
        Some(zone)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dns_nostr_token::DnsNostrToken;

    struct GetDnsNostrTokenStub {}

    impl GetDnsNostrToken for GetDnsNostrTokenStub {
        async fn get_token(&self, _label: &Label) -> Option<DnsNostrToken> {
            None
        }
    }

    #[test]
    fn test_is_valid_dns_nostr_name() {
        let authority = NostrAuthority::new(
            "nostr.dns.name.".parse().unwrap(),
            GetDnsNostrTokenStub {},
            NostrEventsRepository::new("ws://localhost:8080".to_string()),
        );

        let name = "token.nostr.dns.name.".parse().unwrap();
        assert!(authority.is_valid_dns_nostr_name(&name));

        let name = "subdomain.token.nostr.dns.name.".parse().unwrap();
        assert!(authority.is_valid_dns_nostr_name(&name));

        let name = "nostr.dns.name.".parse().unwrap();
        assert!(!authority.is_valid_dns_nostr_name(&name));

        let name = "token.notnostr.dns.name.".parse().unwrap();
        assert!(!authority.is_valid_dns_nostr_name(&name));
    }

    #[test]
    fn test_is_dns_nostr_name() {
        let authority = NostrAuthority::new(
            "nostr.dns.name.".parse().unwrap(),
            GetDnsNostrTokenStub {},
            NostrEventsRepository::new("ws://localhost:8080".to_string()),
        );

        let name = "token.nostr.dns.name.".parse().unwrap();
        assert!(authority.is_valid_dns_nostr_name(&name));

        let name = "subdomain.token.nostr.dns.name.".parse().unwrap();
        assert!(authority.is_valid_dns_nostr_name(&name));

        let name = "nostr.dns.name.".parse().unwrap();
        assert!(!authority.is_valid_dns_nostr_name(&name));

        let name = "token.notnostr.dns.name.".parse().unwrap();
        assert!(!authority.is_valid_dns_nostr_name(&name));
    }

    #[test]
    fn test_extract_token_label() {
        let authority = NostrAuthority::new(
            "nostr.dns.name.".parse().unwrap(),
            GetDnsNostrTokenStub {},
            NostrEventsRepository::new("ws://localhost:8080".to_string()),
        );

        let name = "token.nostr.dns.name.".parse().unwrap();
        let label = authority.extract_token_label(&name);
        assert_eq!(label, Some(Label::from_utf8("token").unwrap()));

        let name = "subdomain.token.nostr.dns.name.".parse().unwrap();
        let label = authority.extract_token_label(&name);
        assert_eq!(label, Some(Label::from_utf8("token").unwrap()));

        let name = "nostr.dns.name.".parse().unwrap();
        let label = authority.extract_token_label(&name);
        assert_eq!(label, None);

        let name = "token.notnostr.dns.name.".parse().unwrap();
        let label = authority.extract_token_label(&name);
        assert_eq!(label, None);
    }

    #[test]
    fn test_extract_zone_name() {
        let authority = NostrAuthority::new(
            "nostr.dns.name.".parse().unwrap(),
            GetDnsNostrTokenStub {},
            NostrEventsRepository::new("ws://localhost:8080".to_string()),
        );

        let name = "token.nostr.dns.name.".parse().unwrap();
        let zone = authority.extract_zone_name(&name);
        assert_eq!(zone, Some("token.nostr.dns.name.".parse().unwrap()));

        let name = "subdomain.token.nostr.dns.name.".parse().unwrap();
        let zone = authority.extract_zone_name(&name);
        assert_eq!(zone, Some("token.nostr.dns.name.".parse().unwrap()));

        let name = "nostr.dns.name.".parse().unwrap();
        let zone = authority.extract_zone_name(&name);
        assert_eq!(zone, None);

        let name = "token.notnostr.dns.name.".parse().unwrap();
        let zone = authority.extract_zone_name(&name);
        assert_eq!(zone, None);
    }
}
