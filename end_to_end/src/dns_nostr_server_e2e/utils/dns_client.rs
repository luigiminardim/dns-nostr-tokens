use super::labels::unique_label;
use crate::utils::config::E2eConfig;
use hickory_proto::{
    op::{Message, MessageType, OpCode, Query, ResponseCode},
    rr::{Name, RData, RecordType},
    serialize::binary::{BinDecodable, BinEncodable},
};
use std::{
    fmt,
    net::SocketAddr,
    str::FromStr,
    time::{Duration, Instant},
};
use tokio::net::UdpSocket;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DnsResponse {
    pub response_code: ResponseCode,
    pub txt_records: Vec<String>,
}

#[derive(Debug)]
pub enum DnsClientError {
    InvalidName(String),
    Encode(String),
    Io(std::io::Error),
    Timeout(Duration),
    Decode(String),
    ResponseIdMismatch { expected: u16, actual: u16 },
}

impl fmt::Display for DnsClientError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidName(message) => write!(formatter, "invalid DNS name: {message}"),
            Self::Encode(message) => write!(formatter, "failed to encode DNS query: {message}"),
            Self::Io(error) => write!(formatter, "DNS UDP I/O error: {error}"),
            Self::Timeout(duration) => write!(formatter, "DNS query timed out after {duration:?}"),
            Self::Decode(message) => write!(formatter, "failed to decode DNS response: {message}"),
            Self::ResponseIdMismatch { expected, actual } => write!(
                formatter,
                "DNS response ID mismatch: expected {expected}, got {actual}"
            ),
        }
    }
}

impl std::error::Error for DnsClientError {}

#[derive(Debug, Clone)]
pub struct DnsClient {
    server_addr: SocketAddr,
}

impl DnsClient {
    pub fn new(server_addr: SocketAddr) -> Self {
        Self { server_addr }
    }

    pub async fn query_txt(&self, fqdn: &str) -> Result<DnsResponse, DnsClientError> {
        self.query_txt_with_timeout(fqdn, Duration::from_secs(3))
            .await
    }

    pub async fn poll_txt_until(
        &self,
        fqdn: &str,
        config: &E2eConfig,
        predicate: impl Fn(&DnsResponse) -> bool,
    ) -> Result<DnsResponse, Result<DnsResponse, DnsClientError>> {
        let deadline = Instant::now() + config.poll_timeout();
        let poll_interval = config.poll_interval();
        loop {
            let last = self.query_txt(fqdn).await;
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

    pub async fn preflight_check(&self, zone_origin: &str) -> Result<(), String> {
        let probe_label = unique_label("preflight");
        let normalized_origin = zone_origin.trim_start_matches('.');
        let fqdn = format!("{probe_label}.{normalized_origin}");
        match self
            .query_txt_with_timeout(&fqdn, Duration::from_secs(1))
            .await
        {
            Ok(response) if response.response_code == ResponseCode::NXDomain => Ok(()),
            Ok(response) => Err(format!(
                "dns_nostr_server at {} returned {:?} for preflight query `{fqdn}` (expected NXDomain); \
                 check that DNS_NOSTR_ZONE_ORIGIN matches `{zone_origin}`",
                self.server_addr, response.response_code
            )),
            Err(DnsClientError::Io(_)) | Err(DnsClientError::Timeout(_)) => Err(format!(
                "dns_nostr_server unreachable at {}",
                self.server_addr
            )),
            Err(other) => Err(format!(
                "dns_nostr_server preflight check failed at {}: {other}",
                self.server_addr
            )),
        }
    }

    async fn query_txt_with_timeout(
        &self,
        fqdn: &str,
        timeout: Duration,
    ) -> Result<DnsResponse, DnsClientError> {
        let query_id: u16 = rand::random();
        let query_bytes = Self::build_txt_query(fqdn, query_id)?;
        let bind_addr = if self.server_addr.is_ipv4() {
            "0.0.0.0:0"
        } else {
            "[::]:0"
        };
        let socket = UdpSocket::bind(bind_addr)
            .await
            .map_err(DnsClientError::Io)?;
        socket
            .send_to(&query_bytes, self.server_addr)
            .await
            .map_err(DnsClientError::Io)?;

        let mut buffer = [0u8; 4096];
        let (bytes_read, _) = tokio::time::timeout(timeout, socket.recv_from(&mut buffer))
            .await
            .map_err(|_| DnsClientError::Timeout(timeout))?
            .map_err(DnsClientError::Io)?;

        Self::decode_txt_response(&buffer[..bytes_read], query_id)
    }

    fn build_txt_query(fqdn: &str, id: u16) -> Result<Vec<u8>, DnsClientError> {
        let name =
            Name::from_str(fqdn).map_err(|error| DnsClientError::InvalidName(error.to_string()))?;
        let mut message = Message::new();
        message
            .set_id(id)
            .set_message_type(MessageType::Query)
            .set_op_code(OpCode::Query)
            .set_recursion_desired(true)
            .add_query(Query::query(name, RecordType::TXT));

        message
            .to_bytes()
            .map_err(|error| DnsClientError::Encode(error.to_string()))
    }

    fn decode_txt_response(bytes: &[u8], expected_id: u16) -> Result<DnsResponse, DnsClientError> {
        let message = Message::from_bytes(bytes)
            .map_err(|error| DnsClientError::Decode(error.to_string()))?;
        if message.id() != expected_id {
            return Err(DnsClientError::ResponseIdMismatch {
                expected: expected_id,
                actual: message.id(),
            });
        }
        let txt_records = message
            .answers()
            .iter()
            .filter_map(|record| match record.data() {
                Some(RData::TXT(txt)) => {
                    let combined: Vec<u8> =
                        txt.iter().flat_map(|chunk| chunk.iter().copied()).collect();
                    Some(String::from_utf8_lossy(&combined).into_owned())
                }
                _ => None,
            })
            .collect();

        Ok(DnsResponse {
            response_code: message.response_code(),
            txt_records,
        })
    }
}

#[cfg(test)]
mod test_dns_client {
    use super::*;
    use hickory_proto::rr::{rdata::TXT, Record};

    #[test]
    fn encodes_txt_query_with_expected_header_and_question() {
        let bytes = DnsClient::build_txt_query("alice.nostr.dns.name.", 0x1234).unwrap();
        let decoded = Message::from_bytes(&bytes).unwrap();
        assert_eq!(decoded.id(), 0x1234);
        assert_eq!(decoded.message_type(), MessageType::Query);
        assert_eq!(decoded.queries().len(), 1);
        assert_eq!(decoded.queries()[0].query_type(), RecordType::TXT);
        assert_eq!(
            decoded.queries()[0].name().to_utf8(),
            "alice.nostr.dns.name."
        );
    }

    #[test]
    fn decodes_txt_records_and_nxdomain_responses() {
        let mut ok_msg = Message::new();
        ok_msg
            .set_id(42)
            .set_message_type(MessageType::Response)
            .set_response_code(ResponseCode::NoError)
            .add_answer(Record::from_rdata(
                Name::from_str("alice.nostr.dns.name.").unwrap(),
                60,
                RData::TXT(TXT::new(vec!["deadbeef".to_string()])),
            ));
        let ok_bytes = ok_msg.to_bytes().unwrap();
        let decoded = DnsClient::decode_txt_response(&ok_bytes, 42).unwrap();
        assert_eq!(decoded.response_code, ResponseCode::NoError);
        assert_eq!(decoded.txt_records, vec!["deadbeef".to_string()]);

        let mut nx_msg = Message::new();
        nx_msg
            .set_id(99)
            .set_message_type(MessageType::Response)
            .set_response_code(ResponseCode::NXDomain);
        let nx_bytes = nx_msg.to_bytes().unwrap();
        let decoded_nx = DnsClient::decode_txt_response(&nx_bytes, 99).unwrap();
        assert_eq!(decoded_nx.response_code, ResponseCode::NXDomain);
        assert!(decoded_nx.txt_records.is_empty());
    }

    #[test]
    fn rejects_mismatched_response_id() {
        let mut msg = Message::new();
        msg.set_id(7).set_message_type(MessageType::Response);
        let bytes = msg.to_bytes().unwrap();
        let error = DnsClient::decode_txt_response(&bytes, 8).unwrap_err();
        assert!(matches!(
            error,
            DnsClientError::ResponseIdMismatch {
                expected: 8,
                actual: 7
            }
        ));
    }
}
