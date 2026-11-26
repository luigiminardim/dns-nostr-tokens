use crate::utils::config::E2eConfig;
use std::{
    io::Write,
    path::Path,
    process::{Command, Output, Stdio},
};

pub const BIP39_VECTOR: &str =
    "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
pub const REGTEST_EXTERNAL_0: &str = "bcrt1q6rz28mcfaxtmd6v789l9rrlrusdprr9pz3cppk";

pub struct WalletCli<'a> {
    config: &'a E2eConfig,
}

impl<'a> WalletCli<'a> {
    pub fn new(config: &'a E2eConfig) -> Self {
        Self { config }
    }

    pub fn run(
        &self,
        wallet_dir: &Path,
        network: Option<&str>,
        args: &[&str],
        stdin_input: Option<&str>,
    ) -> Result<Output, String> {
        let binary = E2eConfig::wallet_binary_path()?;
        let mut command = Command::new(binary);
        command
            .args(args)
            .env("DNS_NOSTR_WALLET_DIR", wallet_dir)
            .env("DNS_NOSTR_BITCOIN_RPC_URL", &self.config.bitcoin_rpc_url)
            .env("DNS_NOSTR_BITCOIN_RPC_USER", &self.config.bitcoin_rpc_user)
            .env(
                "DNS_NOSTR_BITCOIN_RPC_PASSWORD",
                &self.config.bitcoin_rpc_password,
            )
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        match network {
            Some(value) => {
                command.env("DNS_NOSTR_NETWORK", value);
            }
            None => {
                command.env_remove("DNS_NOSTR_NETWORK");
            }
        }

        if let Some(input) = stdin_input {
            command.stdin(Stdio::piped());
            let mut child = command
                .spawn()
                .map_err(|error| format!("failed to spawn dns_nostr_wallet: {error}"))?;
            child
                .stdin
                .as_mut()
                .ok_or_else(|| "failed to open dns_nostr_wallet stdin".to_string())?
                .write_all(input.as_bytes())
                .map_err(|error| format!("failed to write stdin to dns_nostr_wallet: {error}"))?;
            child
                .wait_with_output()
                .map_err(|error| format!("failed to wait for dns_nostr_wallet: {error}"))
        } else {
            command
                .stdin(Stdio::null())
                .output()
                .map_err(|error| format!("failed to execute dns_nostr_wallet: {error}"))
        }
    }

    pub fn run_ok(
        &self,
        wallet_dir: &Path,
        args: &[&str],
        stdin_input: Option<&str>,
    ) -> Result<String, String> {
        let output = self.run(wallet_dir, None, args, stdin_input)?;
        if !output.status.success() {
            return Err(format!(
                "wallet {:?} failed: stderr={}",
                args,
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}
