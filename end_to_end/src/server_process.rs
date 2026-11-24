use crate::{config::E2eConfig, dns_client::DnsClient};
use std::{
    io::Read,
    net::{SocketAddr, UdpSocket},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    thread::JoinHandle,
    time::{Duration, Instant},
};

/// RAII handle for a `dns_nostr_server` child process managed by `end_to_end`.
pub struct ManagedServer {
    child: Child,
    stderr_log: Arc<Mutex<String>>,
    stderr_reader: Option<JoinHandle<()>>,
}

impl ManagedServer {
    pub async fn start(config: &E2eConfig) -> Result<Self, String> {
        Self::ensure_bind_addr_available(config.bind_addr)?;
        let binary_path = E2eConfig::server_binary_path()?;
        let mut child = Command::new(&binary_path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| {
                format!(
                    "failed to spawn dns_nostr_server at `{}`: {error}",
                    binary_path.display()
                )
            })?;

        let stderr_log = Arc::new(Mutex::new(String::new()));
        let stderr_reader = child.stderr.take().map(|stderr| {
            let log_ref = Arc::clone(&stderr_log);
            std::thread::spawn(move || Self::drain_stderr(stderr, log_ref, 16 * 1024))
        });

        let mut server = Self {
            child,
            stderr_log,
            stderr_reader,
        };
        server.wait_until_ready(config).await?;
        Ok(server)
    }

    pub fn captured_stderr(&self) -> String {
        self.stderr_log
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }

    fn ensure_bind_addr_available(bind_addr: SocketAddr) -> Result<(), String> {
        UdpSocket::bind(bind_addr).map(|_| ()).map_err(|error| {
            format!(
                "cannot start dns_nostr_server on `{bind_addr}`: {error} \
                 (stop any running 'just serve' or override DNS_NOSTR_BIND_ADDR)"
            )
        })
    }

    async fn wait_until_ready(&mut self, config: &E2eConfig) -> Result<(), String> {
        let readiness_timeout = Duration::from_secs(10);
        let retry_interval = Duration::from_millis(100);
        let deadline = Instant::now() + readiness_timeout;
        let dns_client = DnsClient::new(config.dns_server_addr);
        let zone_origin = config.zone_origin.to_string();

        loop {
            self.fail_if_exited(config)?;

            match dns_client.preflight_check(&zone_origin).await {
                Ok(()) => {
                    self.fail_if_exited(config)?;
                    return Ok(());
                }
                Err(last_error) => {
                    if Instant::now() >= deadline {
                        let stderr = self.captured_stderr();
                        return Err(format!(
                            "dns_nostr_server did not become ready within {readiness_timeout:?}: {last_error}\nstderr:\n{}",
                            stderr.trim()
                        ));
                    }
                    tokio::time::sleep(retry_interval).await;
                }
            }
        }
    }

    fn fail_if_exited(&mut self, config: &E2eConfig) -> Result<(), String> {
        if let Some(status) = self
            .child
            .try_wait()
            .map_err(|error| format!("failed to poll dns_nostr_server status: {error}"))?
        {
            if let Some(handle) = self.stderr_reader.take() {
                let _ = handle.join();
            }
            let stderr = self.captured_stderr();
            let hint = if stderr.contains("Address already in use") {
                " (stop any running 'just serve' or override DNS_NOSTR_BIND_ADDR)"
            } else {
                ""
            };
            return Err(format!(
                "dns_nostr_server exited before becoming ready (status: {status}, db: `{}`){hint}\nstderr:\n{}",
                config.db_path.display(),
                stderr.trim()
            ));
        }
        Ok(())
    }

    fn drain_stderr(mut reader: impl Read, buffer: Arc<Mutex<String>>, max_bytes: usize) {
        let mut chunk = [0u8; 1024];
        loop {
            match reader.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(bytes_read) => {
                    let text = String::from_utf8_lossy(&chunk[..bytes_read]);
                    if let Ok(mut guard) = buffer.lock() {
                        Self::append_bounded(&mut guard, &text, max_bytes);
                    }
                }
            }
        }
    }

    fn append_bounded(buffer: &mut String, incoming: &str, max_bytes: usize) {
        if max_bytes == 0 {
            buffer.clear();
            return;
        }
        buffer.push_str(incoming);
        if buffer.len() > max_bytes {
            let mut drop_prefix = buffer.len() - max_bytes;
            while drop_prefix < buffer.len() && !buffer.is_char_boundary(drop_prefix) {
                drop_prefix += 1;
            }
            buffer.drain(..drop_prefix);
        }
    }
}

impl Drop for ManagedServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(handle) = self.stderr_reader.take() {
            let _ = handle.join();
        }
    }
}

#[cfg(test)]
mod test_server_process {
    use super::*;

    #[test]
    fn append_bounded_keeps_most_recent_utf8_tail() {
        let mut buffer = String::new();
        ManagedServer::append_bounded(&mut buffer, "abcdef", 4);
        assert_eq!(buffer, "cdef");

        ManagedServer::append_bounded(&mut buffer, "gh", 4);
        assert_eq!(buffer, "efgh");
    }

    #[test]
    fn append_bounded_respects_utf8_char_boundaries() {
        let mut buffer = String::new();
        ManagedServer::append_bounded(&mut buffer, "aébc", 4);
        assert!(buffer.len() <= 4);
        assert!(buffer.ends_with("bc"));
    }

    #[test]
    fn ensure_bind_addr_available_rejects_occupied_udp_port() {
        let occupied = UdpSocket::bind("127.0.0.1:0").unwrap();
        let addr = occupied.local_addr().unwrap();

        let error = ManagedServer::ensure_bind_addr_available(addr).unwrap_err();
        assert!(error.contains("just serve"), "{error}");

        drop(occupied);
        ManagedServer::ensure_bind_addr_available(addr).unwrap();
    }
}
