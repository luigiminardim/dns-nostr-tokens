use bitcoin::Network;
use std::fmt;
use std::path::PathBuf;

pub const NETWORK_VARIABLE: &str = "DNS_NOSTR_NETWORK";
pub const WALLET_DIR_VARIABLE: &str = "DNS_NOSTR_WALLET_DIR";
const DEFAULT_NETWORK: Network = Network::Regtest;
const DEFAULT_WALLET_DIR: &str = "./.data/dns-nostr-wallet";
const DEFAULT_WALLET_DB_NAME: &str = "wallet.sqlite";

/// Wallet settings read from environment variables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentConfig {
    pub network: Network,
    pub wallet_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    UnknownNetwork(String),
}

impl EnvironmentConfig {
    pub fn from_environment() -> Result<Self, ConfigError> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let network = match lookup(NETWORK_VARIABLE) {
            Some(name) => parse_network(&name)?,
            None => DEFAULT_NETWORK,
        };
        let wallet_dir = lookup(WALLET_DIR_VARIABLE)
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(DEFAULT_WALLET_DIR));
        Ok(Self {
            network,
            wallet_dir,
        })
    }

    /// Full path to the SQLite wallet database file.
    pub fn db_path(&self) -> PathBuf {
        self.wallet_dir.join(DEFAULT_WALLET_DB_NAME)
    }
}

fn parse_network(name: &str) -> Result<Network, ConfigError> {
    match name {
        "regtest" => Ok(Network::Regtest),
        "testnet" => Ok(Network::Testnet),
        "mainnet" => Ok(Network::Bitcoin),
        other => Err(ConfigError::UnknownNetwork(other.to_string())),
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownNetwork(name) => write!(
                formatter,
                "unknown network `{name}` in {NETWORK_VARIABLE}; expected regtest, testnet or mainnet"
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
            let config = config_with(&[("DNS_NOSTR_NETWORK", name)]).unwrap();
            assert_eq!(config.network, expected, "network {name}");
        }
    }

    #[test]
    fn defaults_to_regtest() {
        assert_eq!(config_with(&[]).unwrap().network, Network::Regtest);
    }

    #[test]
    fn rejects_unknown_network() {
        let result = config_with(&[("DNS_NOSTR_NETWORK", "dogecoin")]);
        assert_eq!(result, Err(ConfigError::UnknownNetwork("dogecoin".into())));
    }

    #[test]
    fn default_wallet_dir_is_dot_data_dns_nostr_wallet() {
        let config = config_with(&[]).unwrap();
        assert_eq!(config.wallet_dir, PathBuf::from("./.data/dns-nostr-wallet"));
        assert_eq!(
            config.db_path(),
            PathBuf::from("./.data/dns-nostr-wallet/wallet.sqlite")
        );
    }

    #[test]
    fn wallet_dir_env_overrides_default() {
        let config = config_with(&[("DNS_NOSTR_WALLET_DIR", "/somewhere/else")]).unwrap();
        assert_eq!(config.wallet_dir, PathBuf::from("/somewhere/else"));
        assert_eq!(
            config.db_path(),
            PathBuf::from("/somewhere/else/wallet.sqlite")
        );
    }
}
