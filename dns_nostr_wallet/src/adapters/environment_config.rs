use bitcoin::Network;
use std::fmt;
use std::path::PathBuf;

pub const NETWORK_VARIABLE: &str = "DNS_NOSTR_NETWORK";
pub const WALLET_DIR_VARIABLE: &str = "DNS_NOSTR_WALLET_DIR";
pub const BITCOIN_RPC_URL_VARIABLE: &str = "DNS_NOSTR_BITCOIN_RPC_URL";
pub const BITCOIN_RPC_USER_VARIABLE: &str = "DNS_NOSTR_BITCOIN_RPC_USER";
pub const BITCOIN_RPC_PASSWORD_VARIABLE: &str = "DNS_NOSTR_BITCOIN_RPC_PASSWORD";

/// Wallet settings read from environment variables.
#[derive(Clone, PartialEq, Eq)]
pub struct EnvironmentConfig {
    pub network: Network,
    pub wallet_dir: PathBuf,
    pub bitcoin_rpc_url: String,
    pub bitcoin_rpc_user: String,
    pub bitcoin_rpc_password: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    UnknownNetwork(String),
    EmptyValue {
        variable: &'static str,
    },
    InvalidUrl {
        variable: &'static str,
        value: String,
        expected_schemes: &'static str,
    },
}

impl EnvironmentConfig {
    pub fn from_environment() -> Result<Self, ConfigError> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        const DEFAULT_NETWORK: Network = Network::Regtest;
        const DEFAULT_WALLET_DIR: &str = "./.data/dns-nostr-wallet";
        const DEFAULT_BITCOIN_RPC_URL: &str = "http://127.0.0.1:18443";
        const DEFAULT_BITCOIN_RPC_USER: &str = "rpcuser";
        const DEFAULT_BITCOIN_RPC_PASSWORD: &str = "rpcpassword";

        let network = match lookup(NETWORK_VARIABLE) {
            Some(name) => Self::parse_network(&name)?,
            None => DEFAULT_NETWORK,
        };
        let wallet_dir = lookup(WALLET_DIR_VARIABLE)
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(DEFAULT_WALLET_DIR));
        let bitcoin_rpc_url = match lookup(BITCOIN_RPC_URL_VARIABLE) {
            Some(value) => Self::parse_url(
                BITCOIN_RPC_URL_VARIABLE,
                &value,
                &["http://", "https://"],
                "http:// or https://",
            )?,
            None => DEFAULT_BITCOIN_RPC_URL.to_string(),
        };
        let bitcoin_rpc_user = match lookup(BITCOIN_RPC_USER_VARIABLE) {
            Some(value) => Self::require_non_empty(BITCOIN_RPC_USER_VARIABLE, &value)?,
            None => DEFAULT_BITCOIN_RPC_USER.to_string(),
        };
        let bitcoin_rpc_password = match lookup(BITCOIN_RPC_PASSWORD_VARIABLE) {
            Some(value) => Self::require_non_empty_raw(BITCOIN_RPC_PASSWORD_VARIABLE, value)?,
            None => DEFAULT_BITCOIN_RPC_PASSWORD.to_string(),
        };

        Ok(Self {
            network,
            wallet_dir,
            bitcoin_rpc_url,
            bitcoin_rpc_user,
            bitcoin_rpc_password,
        })
    }

    /// Full path to the SQLite wallet database file.
    pub fn db_path(&self) -> PathBuf {
        const DEFAULT_WALLET_DB_NAME: &str = "wallet.sqlite";
        self.wallet_dir.join(DEFAULT_WALLET_DB_NAME)
    }

    fn parse_network(name: &str) -> Result<Network, ConfigError> {
        match name {
            "regtest" => Ok(Network::Regtest),
            "testnet" => Ok(Network::Testnet),
            "mainnet" => Ok(Network::Bitcoin),
            other => Err(ConfigError::UnknownNetwork(other.to_string())),
        }
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
}

impl fmt::Debug for EnvironmentConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EnvironmentConfig")
            .field("network", &self.network)
            .field("wallet_dir", &self.wallet_dir)
            .field("bitcoin_rpc_url", &self.bitcoin_rpc_url)
            .field("bitcoin_rpc_user", &self.bitcoin_rpc_user)
            .field("bitcoin_rpc_password", &"[REDACTED]")
            .finish()
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownNetwork(name) => write!(
                formatter,
                "unknown network `{name}` in {NETWORK_VARIABLE}; expected regtest, testnet or mainnet"
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
    fn parses_known_networks() {
        for (name, expected) in [
            ("regtest", Network::Regtest),
            ("testnet", Network::Testnet),
            ("mainnet", Network::Bitcoin),
        ] {
            let config = config_with(&[(NETWORK_VARIABLE, name)]).unwrap();
            assert_eq!(config.network, expected, "network {name}");
        }
    }

    #[test]
    fn defaults_match_specification() {
        let config = config_with(&[]).unwrap();
        assert_eq!(config.network, Network::Regtest);
        assert_eq!(config.wallet_dir, PathBuf::from("./.data/dns-nostr-wallet"));
        assert_eq!(
            config.db_path(),
            PathBuf::from("./.data/dns-nostr-wallet/wallet.sqlite")
        );
        assert_eq!(config.bitcoin_rpc_url, "http://127.0.0.1:18443");
        assert_eq!(config.bitcoin_rpc_user, "rpcuser");
        assert_eq!(config.bitcoin_rpc_password, "rpcpassword");
    }

    #[test]
    fn rejects_unknown_network() {
        let result = config_with(&[(NETWORK_VARIABLE, "dogecoin")]);
        assert_eq!(result, Err(ConfigError::UnknownNetwork("dogecoin".into())));
    }

    #[test]
    fn overrides_every_variable() {
        let config = config_with(&[
            (NETWORK_VARIABLE, "mainnet"),
            (WALLET_DIR_VARIABLE, "/somewhere/else"),
            (BITCOIN_RPC_URL_VARIABLE, "https://btc.local:8332"),
            (BITCOIN_RPC_USER_VARIABLE, "  alice  "),
            (BITCOIN_RPC_PASSWORD_VARIABLE, "  pw with spaces  "),
        ])
        .unwrap();
        assert_eq!(config.network, Network::Bitcoin);
        assert_eq!(config.wallet_dir, PathBuf::from("/somewhere/else"));
        assert_eq!(
            config.db_path(),
            PathBuf::from("/somewhere/else/wallet.sqlite")
        );
        assert_eq!(config.bitcoin_rpc_url, "https://btc.local:8332");
        assert_eq!(config.bitcoin_rpc_user, "alice");
        assert_eq!(config.bitcoin_rpc_password, "  pw with spaces  ");
    }

    #[test]
    fn rejects_invalid_bitcoin_rpc_urls() {
        for invalid in [
            "http://",
            "https://",
            "http:///path",
            "http://local host:18443",
            "ws://127.0.0.1:18443",
            "127.0.0.1:18443",
        ] {
            assert_eq!(
                config_with(&[(BITCOIN_RPC_URL_VARIABLE, invalid)]),
                Err(ConfigError::InvalidUrl {
                    variable: BITCOIN_RPC_URL_VARIABLE,
                    value: invalid.to_string(),
                    expected_schemes: "http:// or https://",
                })
            );
        }
    }

    #[test]
    fn rejects_empty_or_whitespace_user_and_password() {
        for variable in [BITCOIN_RPC_USER_VARIABLE, BITCOIN_RPC_PASSWORD_VARIABLE] {
            for blank in ["", "   "] {
                assert_eq!(
                    config_with(&[(variable, blank)]),
                    Err(ConfigError::EmptyValue { variable })
                );
            }
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
