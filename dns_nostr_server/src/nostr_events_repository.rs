use std::time::Duration;

pub struct NostrEventsRepository {
    nostr_relay_url: String,
}

impl NostrEventsRepository {
    pub fn new(nostr_relay_url: String) -> Self {
        NostrEventsRepository { nostr_relay_url }
    }

    pub async fn get_last_text_note_from_pubkey(
        &self,
        pubkey: nostr_sdk::PublicKey,
    ) -> Option<nostr_sdk::Event> {
        let nostr_client = nostr_sdk::Client::default();
        if let Err(error) = nostr_client.add_relay(&self.nostr_relay_url).await {
            eprintln!(
                "Failed to add Nostr relay `{}`: {error}",
                self.nostr_relay_url
            );
            nostr_client.shutdown().await;
            return None;
        }
        nostr_client.connect().await;

        let filter = nostr_sdk::Filter::new()
            .author(pubkey)
            .kind(nostr_sdk::Kind::TextNote);
        let fetch_result = nostr_client
            .fetch_events(filter, Duration::from_secs(2))
            .await;
        nostr_client.shutdown().await;

        match fetch_result {
            Ok(text_notes) => text_notes.first().cloned(),
            Err(error) => {
                eprintln!(
                    "Failed to fetch Nostr events for `{pubkey}` from `{}`: {error}",
                    self.nostr_relay_url
                );
                None
            }
        }
    }
}

#[cfg(test)]
mod test_nostr_events_repository {
    use super::*;

    #[tokio::test]
    async fn invalid_relay_url_returns_none_without_panicking() {
        let repository = NostrEventsRepository::new("not a valid relay url".to_string());
        let pubkey = nostr_sdk::Keys::generate().public_key();
        assert_eq!(repository.get_last_text_note_from_pubkey(pubkey).await, None);
    }
}
