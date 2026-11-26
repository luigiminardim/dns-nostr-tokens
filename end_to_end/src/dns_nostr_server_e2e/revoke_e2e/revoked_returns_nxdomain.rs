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
    let label = unique_label("revoked");
    let fqdn = fqdn_for(&label, &config.zone_origin);

    let keys_a = Keys::generate();
    let pubkey_a_hex = keys_a.public_key().to_hex();
    publish_txt_zone_file(&config.nostr_relay_url, &keys_a, &pubkey_a_hex).await?;

    let minted = token_client.mint_token(&label, &keys_a.public_key())?;
    dns_client
        .poll_txt_until(&fqdn, config, |response| {
            response.response_code == ResponseCode::NoError
                && response.txt_records.iter().any(|txt| txt == &pubkey_a_hex)
        })
        .await
        .map_err(|last_dns| {
            format!(
                "label `{label}` did not resolve before revoke (mint_txid={}, height={}, last_dns={last_dns:?})",
                minted.outpoint.txid, minted.block_height
            )
        })?;

    let (revoke_txid, revoke_height) = token_client.revoke_token(&minted)?;

    dns_client
        .poll_txt_until(&fqdn, config, |response| {
            response.response_code == ResponseCode::NXDomain
        })
        .await
        .map_err(|last_dns| {
            format!(
                "revoked label `{label}` ({fqdn}) did not transition to NXDomain \
                 (mint_txid={}, mint_height={}, revoke_txid={}, revoke_height={}, pubkey={}, last_dns={last_dns:?})",
                minted.outpoint.txid,
                minted.block_height,
                revoke_txid,
                revoke_height,
                pubkey_a_hex
            )
        })?;
    Ok(())
}
