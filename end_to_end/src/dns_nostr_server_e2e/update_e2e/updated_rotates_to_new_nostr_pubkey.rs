use super::super::utils::{
    dns_client::DnsClient,
    labels::{fqdn_for, unique_label},
    nostr_publisher::publish_txt_zone_file,
    token_tx::TokenTxClient,
};
use crate::utils::config::E2eConfig;
use hickory_proto::op::ResponseCode;
use nostr_sdk::Keys;

pub async fn run(
    config: &E2eConfig,
    dns_client: &DnsClient,
    token_client: &TokenTxClient<'_>,
) -> Result<(), String> {
    let label = unique_label("updated");
    let fqdn = fqdn_for(&label, &config.zone_origin);

    let keys_a = Keys::generate();
    let pubkey_a_hex = keys_a.public_key().to_hex();
    publish_txt_zone_file(&config.nostr_relay_url, &keys_a, &pubkey_a_hex)
        .await
        .map_err(|error| format!("failed to publish initial zone for `{label}`: {error}"))?;
    let minted = token_client
        .mint_token(&label, &keys_a.public_key())
        .map_err(|error| format!("failed to mint `{label}`: {error}"))?;

    dns_client
        .poll_txt_until(&fqdn, config, |response| {
            response.response_code == ResponseCode::NoError
                && response.txt_records.iter().any(|txt| txt == &pubkey_a_hex)
        })
        .await
        .map_err(|last_dns| {
            format!(
                "initial mint for `{label}` ({fqdn}) did not resolve to `{pubkey_a_hex}` \
                 (mint_txid={}, height={}, last_dns={last_dns:?})",
                minted.outpoint.txid, minted.block_height
            )
        })?;

    let keys_b = Keys::generate();
    let pubkey_b_hex = keys_b.public_key().to_hex();
    publish_txt_zone_file(&config.nostr_relay_url, &keys_b, &pubkey_b_hex)
        .await
        .map_err(|error| format!("failed to publish updated zone for `{label}`: {error}"))?;
    let updated = token_client
        .update_token(&minted, &label, &keys_b.public_key())
        .map_err(|error| format!("failed to update `{label}`: {error}"))?;

    dns_client
        .poll_txt_until(&fqdn, config, |response| {
            response.response_code == ResponseCode::NoError
                && response.txt_records.iter().any(|txt| txt == &pubkey_b_hex)
        })
        .await
        .map_err(|last_dns| {
            format!(
                "updated label `{label}` ({fqdn}) did not rotate from `{pubkey_a_hex}` to `{pubkey_b_hex}` \
                 (mint_txid={}, mint_height={}, update_txid={}, update_height={}, last_dns={last_dns:?})",
                minted.outpoint.txid,
                minted.block_height,
                updated.outpoint.txid,
                updated.block_height
            )
        })?;
    Ok(())
}
