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
    let label = unique_label("first-root");
    let fqdn = fqdn_for(&label, &config.zone_origin);

    let keys_a = Keys::generate();
    let pubkey_a_hex = keys_a.public_key().to_hex();
    let keys_b = Keys::generate();
    let pubkey_b_hex = keys_b.public_key().to_hex();

    publish_txt_zone_file(&config.nostr_relay_url, &keys_a, &pubkey_a_hex).await?;
    publish_txt_zone_file(&config.nostr_relay_url, &keys_b, &pubkey_b_hex).await?;

    let first_mint = token_client.mint_token(&label, &keys_a.public_key())?;
    dns_client
        .poll_txt_until(&fqdn, config, |response| {
            response.response_code == ResponseCode::NoError
                && response.txt_records.iter().any(|txt| txt == &pubkey_a_hex)
        })
        .await
        .map_err(|last_dns| {
            format!(
                "first mint for `{label}` did not resolve to `{pubkey_a_hex}` \
                 (txid={}, height={}, last_dns={last_dns:?})",
                first_mint.outpoint.txid, first_mint.block_height
            )
        })?;

    let competing_mint = token_client.mint_token(&label, &keys_b.public_key())?;

    // Mint an unrelated sequential canary label in the next block and wait for it to resolve,
    // proving BlockchainWatcher has indexed past competing_mint's block without a blind sleep.
    let canary_label = unique_label("canary");
    let canary_fqdn = fqdn_for(&canary_label, &config.zone_origin);
    let canary_keys = Keys::generate();
    let canary_pubkey_hex = canary_keys.public_key().to_hex();
    publish_txt_zone_file(&config.nostr_relay_url, &canary_keys, &canary_pubkey_hex).await?;
    let canary_mint = token_client.mint_token(&canary_label, &canary_keys.public_key())?;

    dns_client
        .poll_txt_until(&canary_fqdn, config, |response| {
            response.response_code == ResponseCode::NoError
                && response
                    .txt_records
                    .iter()
                    .any(|txt| txt == &canary_pubkey_hex)
        })
        .await
        .map_err(|last_dns| {
            format!(
                "canary label `{canary_label}` did not resolve after competing mint \
                 (canary_txid={}, canary_height={}, last_dns={last_dns:?})",
                canary_mint.outpoint.txid, canary_mint.block_height
            )
        })?;

    let final_response = dns_client
        .query_txt(&fqdn)
        .await
        .map_err(|error| format!("DNS query failed for `{label}` after canary: {error}"))?;
    if final_response.response_code != ResponseCode::NoError {
        return Err(format!(
            "label `{label}` should stay NoError after competing mint \
             (first_txid={}, first_height={}, competing_txid={}, competing_height={}, response={final_response:?})",
            first_mint.outpoint.txid,
            first_mint.block_height,
            competing_mint.outpoint.txid,
            competing_mint.block_height
        ));
    }
    if final_response.txt_records != vec![pubkey_a_hex.clone()] {
        return Err(format!(
            "first-is-root violated for `{label}`: expected `{pubkey_a_hex}` instead of competing `{pubkey_b_hex}` \
             (first_txid={}, first_height={}, competing_txid={}, competing_height={}, response={final_response:?})",
            first_mint.outpoint.txid,
            first_mint.block_height,
            competing_mint.outpoint.txid,
            competing_mint.block_height
        ));
    }
    Ok(())
}
