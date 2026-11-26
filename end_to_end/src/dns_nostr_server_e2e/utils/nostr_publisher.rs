use nostr_sdk::{Client, EventBuilder, EventId, Keys};

/// Publishes a minimal RFC-1034 zone file with an apex `TXT` record as a Nostr text note.
pub async fn publish_txt_zone_file(
    relay_url: &str,
    keys: &Keys,
    txt_value: &str,
) -> Result<EventId, String> {
    let zone_content = format!(
        "$TTL 60\n@ IN SOA ns1.nostr.dns.name. hostmaster.nostr.dns.name. ( 1 3600 600 86400 60 )\n@ IN TXT \"{txt_value}\"\n"
    );
    let client = Client::new(keys.clone());
    if let Err(error) = client.add_relay(relay_url).await {
        client.shutdown().await;
        return Err(format!("failed to add Nostr relay `{relay_url}`: {error}"));
    }
    client.connect().await;

    let send_result = client
        .send_event_builder(EventBuilder::text_note(zone_content))
        .await;
    client.shutdown().await;

    let output = send_result
        .map_err(|error| format!("failed to publish zone file to `{relay_url}`: {error}"))?;
    if output.success.is_empty() {
        return Err(format!(
            "Nostr relay `{relay_url}` did not accept event `{}` (failed: {:?})",
            output.val, output.failed
        ));
    }
    Ok(output.val)
}
