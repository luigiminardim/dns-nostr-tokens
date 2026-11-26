use dns_nostr_server::environment_config::{ConfigError, EnvironmentConfig};
use hickory_proto::rr::LowerName;
use std::{
    fmt,
    net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6},
    path::{Path, PathBuf},
    time::Duration,
};

/// End-to-end test configuration derived from `dns_nostr_server::environment_config::EnvironmentConfig`.
#[derive(Clone, PartialEq, Eq)]
pub struct E2eConfig {
    pub bind_addr: SocketAddr,
    pub dns_server_addr: SocketAddr,
    pub zone_origin: LowerName,
    pub db_path: PathBuf,
    pub bitcoin_rpc_url: String,
    pub bitcoin_rpc_user: String,
    pub bitcoin_rpc_password: String,
    pub nostr_relay_url: String,
    pub sync_interval: Duration,
}

impl E2eConfig {
    pub fn from_environment() -> Result<Self, ConfigError> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let server_config = EnvironmentConfig::from_lookup(lookup)?;
        Ok(Self {
            bind_addr: server_config.bind_addr,
            dns_server_addr: Self::map_bind_addr_to_client_addr(server_config.bind_addr),
            zone_origin: server_config.zone_origin,
            db_path: server_config.db_path,
            bitcoin_rpc_url: server_config.bitcoin_rpc_url,
            bitcoin_rpc_user: server_config.bitcoin_rpc_user,
            bitcoin_rpc_password: server_config.bitcoin_rpc_password,
            nostr_relay_url: server_config.nostr_relay_url,
            sync_interval: server_config.sync_interval,
        })
    }

    pub fn poll_timeout(&self) -> Duration {
        Duration::from_secs(5).max(self.sync_interval.saturating_mul(5))
    }

    pub fn poll_interval(&self) -> Duration {
        Duration::from_millis(250)
            .min(self.sync_interval / 2)
            .max(Duration::from_millis(1))
    }

    pub fn server_binary_path() -> Result<PathBuf, String> {
        Self::binary_path("dns_nostr_server")
    }

    pub fn wallet_binary_path() -> Result<PathBuf, String> {
        Self::binary_path("dns_nostr_wallet")
    }

    fn binary_path(binary_name: &str) -> Result<PathBuf, String> {
        let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .ok_or_else(|| "unable to resolve workspace root directory".to_string())?;

        let profile_dir = Self::profile_dir_from_current_exe().unwrap_or_else(|| {
            match std::env::var("CARGO_TARGET_DIR") {
                Ok(target_dir) if !target_dir.trim().is_empty() => {
                    let target_path = PathBuf::from(target_dir.trim());
                    let anchored = if target_path.is_absolute() {
                        target_path
                    } else {
                        workspace_root.join(target_path)
                    };
                    anchored.join("debug")
                }
                _ => workspace_root.join("target").join("debug"),
            }
        });

        let binary_path =
            profile_dir.join(format!("{binary_name}{}", std::env::consts::EXE_SUFFIX));
        if binary_path.is_file() {
            Ok(binary_path)
        } else {
            Err(format!(
                "{binary_name} binary not found at `{}`; build it with `cargo build -p {binary_name}` or run `just test-e2e`",
                binary_path.display()
            ))
        }
    }

    fn map_bind_addr_to_client_addr(bind_addr: SocketAddr) -> SocketAddr {
        match bind_addr {
            SocketAddr::V4(v4) if v4.ip().is_unspecified() => {
                SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, v4.port()))
            }
            SocketAddr::V6(v6) if v6.ip().is_unspecified() => SocketAddr::V6(SocketAddrV6::new(
                Ipv6Addr::LOCALHOST,
                v6.port(),
                v6.flowinfo(),
                v6.scope_id(),
            )),
            other => other,
        }
    }

    fn profile_dir_from_current_exe() -> Option<PathBuf> {
        let exe = std::env::current_exe().ok()?;
        let parent = exe.parent()?;
        if parent.file_name().and_then(|name| name.to_str()) == Some("deps") {
            parent.parent().map(Path::to_path_buf)
        } else {
            Some(parent.to_path_buf())
        }
    }
}

impl fmt::Debug for E2eConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("E2eConfig")
            .field("dns_server_addr", &self.dns_server_addr)
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

#[cfg(test)]
mod test_config {
    use super::*;
    use dns_nostr_server::environment_config::{
        BIND_ADDR_VARIABLE, BITCOIN_RPC_PASSWORD_VARIABLE, BITCOIN_RPC_URL_VARIABLE,
        DB_PATH_VARIABLE, NOSTR_RELAY_URL_VARIABLE, SYNC_INTERVAL_MS_VARIABLE,
        ZONE_ORIGIN_VARIABLE,
    };

    fn config_with(variables: &[(&str, &str)]) -> Result<E2eConfig, ConfigError> {
        E2eConfig::from_lookup(|name| {
            variables
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| value.to_string())
        })
    }

    #[test]
    fn defaults_map_wildcard_ipv4_to_loopback_and_set_timings() {
        let config = config_with(&[]).unwrap();
        assert_eq!(config.dns_server_addr, "127.0.0.1:1053".parse().unwrap());
        assert_eq!(config.zone_origin, "nostr.dns.name.".parse().unwrap());
        assert_eq!(config.db_path, PathBuf::from("./data/name-tokens.sqlite"));
        assert_eq!(config.bitcoin_rpc_url, "http://127.0.0.1:18443");
        assert_eq!(config.nostr_relay_url, "ws://localhost:8080");
        assert_eq!(config.sync_interval, Duration::from_millis(1000));
        assert_eq!(config.poll_timeout(), Duration::from_secs(5));
        assert_eq!(config.poll_interval(), Duration::from_millis(250));
    }

    #[test]
    fn maps_wildcard_ipv6_and_preserves_explicit_client_addr() {
        let v6_config = config_with(&[(BIND_ADDR_VARIABLE, "[::]:2053")]).unwrap();
        assert_eq!(v6_config.dns_server_addr, "[::1]:2053".parse().unwrap());

        let explicit_config = config_with(&[(BIND_ADDR_VARIABLE, "127.0.0.2:3053")]).unwrap();
        assert_eq!(
            explicit_config.dns_server_addr,
            "127.0.0.2:3053".parse().unwrap()
        );
    }

    #[test]
    fn overrides_and_scales_poll_timings() {
        let fast = config_with(&[
            (ZONE_ORIGIN_VARIABLE, "test.example"),
            (DB_PATH_VARIABLE, "/tmp/custom.sqlite"),
            (BITCOIN_RPC_URL_VARIABLE, "http://127.0.0.1:19443"),
            (NOSTR_RELAY_URL_VARIABLE, "ws://127.0.0.1:9090"),
            (SYNC_INTERVAL_MS_VARIABLE, "200"),
        ])
        .unwrap();
        assert_eq!(fast.zone_origin, "test.example.".parse().unwrap());
        assert_eq!(fast.db_path, PathBuf::from("/tmp/custom.sqlite"));
        assert_eq!(fast.poll_timeout(), Duration::from_secs(5));
        assert_eq!(fast.poll_interval(), Duration::from_millis(100));

        let slow = config_with(&[(SYNC_INTERVAL_MS_VARIABLE, "3000")]).unwrap();
        assert_eq!(slow.poll_timeout(), Duration::from_secs(15));
        assert_eq!(slow.poll_interval(), Duration::from_millis(250));
    }

    #[test]
    fn debug_redacts_bitcoin_rpc_password() {
        let config = config_with(&[(BITCOIN_RPC_PASSWORD_VARIABLE, "e2e-secret-pw")]).unwrap();
        let formatted = format!("{config:?}");
        assert!(formatted.contains("[REDACTED]"), "{formatted}");
        assert!(!formatted.contains("e2e-secret-pw"), "{formatted}");
    }
}
