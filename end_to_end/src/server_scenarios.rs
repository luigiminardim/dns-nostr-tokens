use crate::{
    bitcoin_token_tx::RegtestTokenClient,
    config::E2eConfig,
    dns_client::{DnsClient, DnsClientError, DnsResponse},
    labels::unique_label,
    nostr_publisher::publish_txt_zone_file,
};
use hickory_proto::op::ResponseCode;
use nostr_sdk::Keys;
use std::time::Instant;

pub async fn run_all(
    config: &E2eConfig,
    dns_client: &DnsClient,
    bitcoin_client: &RegtestTokenClient,
) -> Result<(), (&'static str, String)> {
    run_step(
        "server::not_found_returns_nxdomain",
        not_found_returns_nxdomain(config, dns_client).await,
    )?;
    run_step(
        "server::minted_resolves_txt_pubkey",
        minted_resolves_txt_pubkey(config, dns_client, bitcoin_client).await,
    )?;
    run_step(
        "server::updated_rotates_to_new_nostr_pubkey",
        updated_rotates_to_new_nostr_pubkey(config, dns_client, bitcoin_client).await,
    )?;
    run_step(
        "server::first_is_root_ignores_competing_mint",
        first_is_root_ignores_competing_mint(config, dns_client, bitcoin_client).await,
    )?;
    run_step(
        "server::revoked_returns_nxdomain",
        revoked_returns_nxdomain(config, dns_client, bitcoin_client).await,
    )?;
    Ok(())
}

fn run_step(name: &'static str, result: Result<(), String>) -> Result<(), (&'static str, String)> {
    match result {
        Ok(()) => {
            println!("[PASS] {name}");
            Ok(())
        }
        Err(error) => Err((name, error)),
    }
}

async fn poll_dns_until(
    dns_client: &DnsClient,
    fqdn: &str,
    config: &E2eConfig,
    predicate: impl Fn(&DnsResponse) -> bool,
) -> Result<DnsResponse, Result<DnsResponse, DnsClientError>> {
    let deadline = Instant::now() + config.poll_timeout();
    let poll_interval = config.poll_interval();
    loop {
        let last = dns_client.query_txt(fqdn).await;
        if let Ok(ref response) = last {
            if predicate(response) {
                return Ok(response.clone());
            }
        }
        if Instant::now() >= deadline {
            return Err(last);
        }
        tokio::time::sleep(poll_interval).await;
    }
}

fn fqdn_for(label: &dns_nostr_token::label::Label, config: &E2eConfig) -> String {
    format!("{label}.{}", config.zone_origin)
}

async fn not_found_returns_nxdomain(
    config: &E2eConfig,
    dns_client: &DnsClient,
) -> Result<(), String> {
    let label = unique_label("notfound");
    let fqdn = fqdn_for(&label, config);

    let response = dns_client
        .query_txt(&fqdn)
        .await
        .map_err(|error| format!("DNS query failed for unminted label `{label}` ({fqdn}): {error}"))?;

    if response.response_code != ResponseCode::NXDomain {
        return Err(format!(
            "expected NXDomain for unminted label `{label}` ({fqdn}), got response={response:?}"
        ));
    }
    if !response.txt_records.is_empty() {
        return Err(format!(
            "expected no TXT records for unminted label `{label}` ({fqdn}), got response={response:?}"
        ));
    }
    Ok(())
}

async fn minted_resolves_txt_pubkey(
    config: &E2eConfig,
    dns_client: &DnsClient,
    bitcoin_client: &RegtestTokenClient,
) -> Result<(), String> {
    let label = unique_label("minted");
    let fqdn = fqdn_for(&label, config);
    let keys_a = Keys::generate();
    let pubkey_a_hex = keys_a.public_key().to_hex();

    publish_txt_zone_file(&config.nostr_relay_url, &keys_a, &pubkey_a_hex)
        .await
        .map_err(|error| format!("failed to publish Nostr zone for `{label}` ({pubkey_a_hex}): {error}"))?;
    let minted = bitcoin_client
        .mint_token(&label, &keys_a.public_key())
        .map_err(|error| format!("failed to mint `{label}`: {error}"))?;

    poll_dns_until(dns_client, &fqdn, config, |response| {
        response.response_code == ResponseCode::NoError
            && response.txt_records.iter().any(|txt| txt == &pubkey_a_hex)
    })
    .await
    .map_err(|last_dns| {
        format!(
            "minted label `{label}` ({fqdn}) did not resolve to `{pubkey_a_hex}` \
             (txid={}, height={}, last_dns={last_dns:?})",
            minted.outpoint.txid, minted.block_height
        )
    })?;
    Ok(())
}

async fn updated_rotates_to_new_nostr_pubkey(
    config: &E2eConfig,
    dns_client: &DnsClient,
    bitcoin_client: &RegtestTokenClient,
) -> Result<(), String> {
    let label = unique_label("updated");
    let fqdn = fqdn_for(&label, config);

    let keys_a = Keys::generate();
    let pubkey_a_hex = keys_a.public_key().to_hex();
    publish_txt_zone_file(&config.nostr_relay_url, &keys_a, &pubkey_a_hex)
        .await
        .map_err(|error| format!("failed to publish initial zone for `{label}`: {error}"))?;
    let minted = bitcoin_client
        .mint_token(&label, &keys_a.public_key())
        .map_err(|error| format!("failed to mint `{label}`: {error}"))?;

    poll_dns_until(dns_client, &fqdn, config, |response| {
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
    let updated = bitcoin_client
        .update_token(&minted, &label, &keys_b.public_key())
        .map_err(|error| format!("failed to update `{label}`: {error}"))?;

    poll_dns_until(dns_client, &fqdn, config, |response| {
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

async fn first_is_root_ignores_competing_mint(
    config: &E2eConfig,
    dns_client: &DnsClient,
    bitcoin_client: &RegtestTokenClient,
) -> Result<(), String> {
    let label = unique_label("first-root");
    let fqdn = fqdn_for(&label, config);

    let keys_a = Keys::generate();
    let pubkey_a_hex = keys_a.public_key().to_hex();
    let keys_b = Keys::generate();
    let pubkey_b_hex = keys_b.public_key().to_hex();

    publish_txt_zone_file(&config.nostr_relay_url, &keys_a, &pubkey_a_hex).await?;
    publish_txt_zone_file(&config.nostr_relay_url, &keys_b, &pubkey_b_hex).await?;

    let first_mint = bitcoin_client.mint_token(&label, &keys_a.public_key())?;
    poll_dns_until(dns_client, &fqdn, config, |response| {
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

    let competing_mint = bitcoin_client.mint_token(&label, &keys_b.public_key())?;

    // Mint an unrelated sequential canary label in the next block and wait for it to resolve,
    // proving BlockchainWatcher has indexed past competing_mint's block without a blind sleep.
    let canary_label = unique_label("canary");
    let canary_fqdn = fqdn_for(&canary_label, config);
    let canary_keys = Keys::generate();
    let canary_pubkey_hex = canary_keys.public_key().to_hex();
    publish_txt_zone_file(&config.nostr_relay_url, &canary_keys, &canary_pubkey_hex).await?;
    let canary_mint = bitcoin_client.mint_token(&canary_label, &canary_keys.public_key())?;

    poll_dns_until(dns_client, &canary_fqdn, config, |response| {
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

async fn revoked_returns_nxdomain(
    config: &E2eConfig,
    dns_client: &DnsClient,
    bitcoin_client: &RegtestTokenClient,
) -> Result<(), String> {
    let label = unique_label("revoked");
    let fqdn = fqdn_for(&label, config);

    let keys_a = Keys::generate();
    let pubkey_a_hex = keys_a.public_key().to_hex();
    publish_txt_zone_file(&config.nostr_relay_url, &keys_a, &pubkey_a_hex).await?;

    let minted = bitcoin_client.mint_token(&label, &keys_a.public_key())?;
    poll_dns_until(dns_client, &fqdn, config, |response| {
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

    let (revoke_txid, revoke_height) = bitcoin_client.revoke_token(&minted)?;

    poll_dns_until(dns_client, &fqdn, config, |response| {
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
