use hickory_server::proto::rr::{LowerName, Name};
use std::{fmt, net::SocketAddr, path::PathBuf, str::FromStr, time::Duration};

pub const BIND_ADDR_VARIABLE: &str = "DNS_NOSTR_BIND_ADDR";
pub const ZONE_ORIGIN_VARIABLE: &str = "DNS_NOSTR_ZONE_ORIGIN";
pub const DB_PATH_VARIABLE: &str = "DNS_NOSTR_DB_PATH";
pub const BITCOIN_RPC_URL_VARIABLE: &str = "DNS_NOSTR_BITCOIN_RPC_URL";
pub const BITCOIN_RPC_USER_VARIABLE: &str = "DNS_NOSTR_BITCOIN_RPC_USER";
pub const BITCOIN_RPC_PASSWORD_VARIABLE: &str = "DNS_NOSTR_BITCOIN_RPC_PASSWORD";
pub const NOSTR_RELAY_URL_VARIABLE: &str = "DNS_NOSTR_NOSTR_RELAY_URL";
pub const SYNC_INTERVAL_MS_VARIABLE: &str = "DNS_NOSTR_SYNC_INTERVAL_MS";

/// Server settings read and validated from `DNS_NOSTR_*` environment variables.
#[derive(Clone, PartialEq, Eq)]
pub struct EnvironmentConfig {
    pub bind_addr: SocketAddr,
    pub zone_origin: LowerName,
    pub db_path: PathBuf,
    pub bitcoin_rpc_url: String,
    pub bitcoin_rpc_user: String,
    pub bitcoin_rpc_password: String,
    pub nostr_relay_url: String,
    pub sync_interval: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    InvalidBindAddr {
        value: String,
    },
    InvalidZoneOrigin {
        value: String,
    },
    EmptyValue {
        variable: &'static str,
    },
    InvalidUrl {
        variable: &'static str,
        value: String,
        expected_schemes: &'static str,
    },
    InvalidSyncInterval {
        value: String,
    },
}

impl EnvironmentConfig {
    pub fn from_environment() -> Result<Self, ConfigError> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let bind_addr = match lookup(BIND_ADDR_VARIABLE) {
            Some(value) => Self::parse_bind_addr(&value)?,
            None => SocketAddr::from(([0, 0, 0, 0], 1053)),
        };
        let zone_origin = match lookup(ZONE_ORIGIN_VARIABLE) {
            Some(value) => Self::parse_zone_origin(&value)?,
            None => Self::parse_zone_origin("nostr.dns.name.")?,
        };
        let db_path = match lookup(DB_PATH_VARIABLE) {
            Some(value) => PathBuf::from(Self::require_non_empty(DB_PATH_VARIABLE, &value)?),
            None => PathBuf::from("./data/name-tokens.sqlite"),
        };
        let bitcoin_rpc_url = match lookup(BITCOIN_RPC_URL_VARIABLE) {
            Some(value) => Self::parse_url(
                BITCOIN_RPC_URL_VARIABLE,
                &value,
                &["http://", "https://"],
                "http:// or https://",
            )?,
            None => "http://127.0.0.1:18443".to_string(),
        };
        let bitcoin_rpc_user = match lookup(BITCOIN_RPC_USER_VARIABLE) {
            Some(value) => Self::require_non_empty(BITCOIN_RPC_USER_VARIABLE, &value)?,
            None => "rpcuser".to_string(),
        };
        let bitcoin_rpc_password = match lookup(BITCOIN_RPC_PASSWORD_VARIABLE) {
            Some(value) => Self::require_non_empty_raw(BITCOIN_RPC_PASSWORD_VARIABLE, value)?,
            None => "rpcpassword".to_string(),
        };
        let nostr_relay_url = match lookup(NOSTR_RELAY_URL_VARIABLE) {
            Some(value) => Self::parse_url(
                NOSTR_RELAY_URL_VARIABLE,
                &value,
                &["ws://", "wss://"],
                "ws:// or wss://",
            )?,
            None => "ws://localhost:8080".to_string(),
        };
        let sync_interval = match lookup(SYNC_INTERVAL_MS_VARIABLE) {
            Some(value) => Self::parse_sync_interval(&value)?,
            None => Duration::from_millis(1000),
        };

        Ok(Self {
            bind_addr,
            zone_origin,
            db_path,
            bitcoin_rpc_url,
            bitcoin_rpc_user,
            bitcoin_rpc_password,
            nostr_relay_url,
            sync_interval,
        })
    }

    fn parse_bind_addr(raw: &str) -> Result<SocketAddr, ConfigError> {
        let trimmed = raw.trim();
        trimmed
            .parse::<SocketAddr>()
            .map_err(|_| ConfigError::InvalidBindAddr {
                value: raw.to_string(),
            })
    }

    fn parse_zone_origin(raw: &str) -> Result<LowerName, ConfigError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed == "." {
            return Err(ConfigError::InvalidZoneOrigin {
                value: raw.to_string(),
            });
        }
        let fqdn = if trimmed.ends_with('.') {
            trimmed.to_string()
        } else {
            format!("{trimmed}.")
        };
        let parsed = Name::from_str(&fqdn).map_err(|_| ConfigError::InvalidZoneOrigin {
            value: raw.to_string(),
        })?;
        if !parsed.is_fqdn() || parsed.num_labels() < 1 {
            return Err(ConfigError::InvalidZoneOrigin {
                value: raw.to_string(),
            });
        }
        Ok(LowerName::from(parsed))
    }

    fn require_non_empty(variable: &'static str, raw: &str) -> Result<String, ConfigError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(ConfigError::EmptyValue { variable });
        }
        Ok(trimmed.to_string())
    }

    fn require_non_empty_raw(variable: &'static str, raw: String) -> Result<String, ConfigError> {
        if raw.trim().is_empty() {
            return Err(ConfigError::EmptyValue { variable });
        }
        Ok(raw)
    }

    fn parse_url(
        variable: &'static str,
        raw: &str,
        allowed_schemes: &[&str],
        expected_schemes: &'static str,
    ) -> Result<String, ConfigError> {
        let trimmed = raw.trim();
        let rest = allowed_schemes
            .iter()
            .find_map(|scheme| trimmed.strip_prefix(scheme));
        let Some(after_scheme) = rest else {
            return Err(ConfigError::InvalidUrl {
                variable,
                value: raw.to_string(),
                expected_schemes,
            });
        };
        let authority = after_scheme
            .split(['/', '?', '#'])
            .next()
            .unwrap_or_default();
        if authority.is_empty() || authority.chars().any(char::is_whitespace) {
            return Err(ConfigError::InvalidUrl {
                variable,
                value: raw.to_string(),
                expected_schemes,
            });
        }
        Ok(trimmed.to_string())
    }

    fn parse_sync_interval(raw: &str) -> Result<Duration, ConfigError> {
        let trimmed = raw.trim();
        match trimmed.parse::<u64>() {
            Ok(ms) if ms >= 1 => Ok(Duration::from_millis(ms)),
            _ => Err(ConfigError::InvalidSyncInterval {
                value: raw.to_string(),
            }),
        }
    }
}

impl fmt::Debug for EnvironmentConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EnvironmentConfig")
            .field("bind_addr", &self.bind_addr)
            .field("zone_origin", &self.zone_origin)
            .field("db_path", &self.db_path)
            .field("bitcoin_rpc_url", &self.bitcoin_rpc_url)
            .field("bitcoin_rpc_user", &self.bitcoin_rpc_user)
            .field("bitcoin_rpc_password", &"[REDACTED]")
            .field("nostr_relay_url", &self.nostr_relay_url)
            .field("sync_interval", &self.sync_interval)
            .finish()
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBindAddr { value } => write!(
                formatter,
                "invalid socket address `{value}` in {BIND_ADDR_VARIABLE}"
            ),
            Self::InvalidZoneOrigin { value } => write!(
                formatter,
                "invalid zone origin `{value}` in {ZONE_ORIGIN_VARIABLE}; expected a non-root DNS name"
            ),
            Self::EmptyValue { variable } => {
                write!(formatter, "{variable} must not be empty")
            }
            Self::InvalidUrl {
                variable,
                value,
                expected_schemes,
            } => write!(
                formatter,
                "invalid URL `{value}` in {variable}; expected {expected_schemes} with a non-empty host"
            ),
            Self::InvalidSyncInterval { value } => write!(
                formatter,
                "invalid sync interval `{value}` in {SYNC_INTERVAL_MS_VARIABLE}; expected an integer >= 1 (milliseconds)"
            ),
        }
    }
}

impl std::error::Error for ConfigError {}

#[cfg(test)]
mod test_environment_config {
    use super::*;

    fn config_with(variables: &[(&str, &str)]) -> Result<EnvironmentConfig, ConfigError> {
        EnvironmentConfig::from_lookup(|name| {
            variables
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| value.to_string())
        })
    }

    #[test]
    fn defaults_match_specification() {
        let config = config_with(&[]).unwrap();
        assert_eq!(config.bind_addr, "0.0.0.0:1053".parse().unwrap());
        assert_eq!(config.zone_origin, "nostr.dns.name.".parse().unwrap());
        assert_eq!(config.db_path, PathBuf::from("./data/name-tokens.sqlite"));
        assert_eq!(config.bitcoin_rpc_url, "http://127.0.0.1:18443");
        assert_eq!(config.bitcoin_rpc_user, "rpcuser");
        assert_eq!(config.bitcoin_rpc_password, "rpcpassword");
        assert_eq!(config.nostr_relay_url, "ws://localhost:8080");
        assert_eq!(config.sync_interval, Duration::from_millis(1000));
    }

    #[test]
    fn overrides_every_variable() {
        let config = config_with(&[
            (BIND_ADDR_VARIABLE, "127.0.0.1:2053"),
            (ZONE_ORIGIN_VARIABLE, "custom.example.org."),
            (DB_PATH_VARIABLE, "/tmp/custom.sqlite"),
            (BITCOIN_RPC_URL_VARIABLE, "https://btc.local:8332"),
            (BITCOIN_RPC_USER_VARIABLE, "alice"),
            (BITCOIN_RPC_PASSWORD_VARIABLE, "super-secret"),
            (NOSTR_RELAY_URL_VARIABLE, "wss://relay.example.org"),
            (SYNC_INTERVAL_MS_VARIABLE, "250"),
        ])
        .unwrap();

        assert_eq!(config.bind_addr, "127.0.0.1:2053".parse().unwrap());
        assert_eq!(config.zone_origin, "custom.example.org.".parse().unwrap());
        assert_eq!(config.db_path, PathBuf::from("/tmp/custom.sqlite"));
        assert_eq!(config.bitcoin_rpc_url, "https://btc.local:8332");
        assert_eq!(config.bitcoin_rpc_user, "alice");
        assert_eq!(config.bitcoin_rpc_password, "super-secret");
        assert_eq!(config.nostr_relay_url, "wss://relay.example.org");
        assert_eq!(config.sync_interval, Duration::from_millis(250));
    }

    #[test]
    fn normalizes_zone_origin_trailing_dot_and_rejects_empty_or_root() {
        let normalized = config_with(&[(ZONE_ORIGIN_VARIABLE, "  Nostr.Example.Net ")]).unwrap();
        assert_eq!(normalized.zone_origin, "nostr.example.net.".parse().unwrap());

        for invalid in ["", "   ", "."] {
            assert_eq!(
                config_with(&[(ZONE_ORIGIN_VARIABLE, invalid)]),
                Err(ConfigError::InvalidZoneOrigin {
                    value: invalid.to_string(),
                })
            );
        }
    }

    #[test]
    fn rejects_invalid_bind_addr() {
        assert_eq!(
            config_with(&[(BIND_ADDR_VARIABLE, "not-a-socket")]),
            Err(ConfigError::InvalidBindAddr {
                value: "not-a-socket".to_string(),
            })
        );
    }

    #[test]
    fn rejects_bare_and_invalid_urls() {
        for invalid in ["http://", "https://", "http:///path", "ws://localhost:18443", "127.0.0.1:18443"] {
            assert_eq!(
                config_with(&[(BITCOIN_RPC_URL_VARIABLE, invalid)]),
                Err(ConfigError::InvalidUrl {
                    variable: BITCOIN_RPC_URL_VARIABLE,
                    value: invalid.to_string(),
                    expected_schemes: "http:// or https://",
                })
            );
        }

        for invalid in ["ws://", "wss://", "wss:///events", "http://localhost:8080", "localhost:8080"] {
            assert_eq!(
                config_with(&[(NOSTR_RELAY_URL_VARIABLE, invalid)]),
                Err(ConfigError::InvalidUrl {
                    variable: NOSTR_RELAY_URL_VARIABLE,
                    value: invalid.to_string(),
                    expected_schemes: "ws:// or wss://",
                })
            );
        }
    }

    #[test]
    fn rejects_whitespace_only_values() {
        for variable in [
            DB_PATH_VARIABLE,
            BITCOIN_RPC_USER_VARIABLE,
            BITCOIN_RPC_PASSWORD_VARIABLE,
        ] {
            for blank in ["", "   "] {
                assert_eq!(
                    config_with(&[(variable, blank)]),
                    Err(ConfigError::EmptyValue { variable })
                );
            }
        }
    }

    #[test]
    fn rejects_zero_or_non_numeric_sync_interval() {
        for invalid in ["0", "-10", "abc", ""] {
            assert_eq!(
                config_with(&[(SYNC_INTERVAL_MS_VARIABLE, invalid)]),
                Err(ConfigError::InvalidSyncInterval {
                    value: invalid.to_string(),
                })
            );
        }
    }

    #[test]
    fn debug_redacts_bitcoin_rpc_password() {
        let config = config_with(&[(BITCOIN_RPC_PASSWORD_VARIABLE, "top-secret-pw")]).unwrap();
        let formatted = format!("{config:?}");
        assert!(formatted.contains("[REDACTED]"), "{formatted}");
        assert!(!formatted.contains("top-secret-pw"), "{formatted}");
    }
}
