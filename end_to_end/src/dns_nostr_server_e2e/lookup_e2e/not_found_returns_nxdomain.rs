use super::super::utils::{
    dns_client::DnsClient,
    labels::{fqdn_for, unique_label},
};
use crate::utils::config::E2eConfig;
use hickory_proto::op::ResponseCode;

pub async fn run(config: &E2eConfig, dns_client: &DnsClient) -> Result<(), String> {
    let label = unique_label("notfound");
    let fqdn = fqdn_for(&label, &config.zone_origin);

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
