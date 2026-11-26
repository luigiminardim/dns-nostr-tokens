use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

pub struct TempWalletDir(PathBuf);

impl TempWalletDir {
    pub fn new(test_name: &str) -> Result<Self, String> {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("system clock before UNIX_EPOCH: {error}"))?
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "dns_nostr_wallet_e2e_{test_name}_{}_{}_{}",
            std::process::id(),
            nanos,
            seq
        ));
        std::fs::create_dir_all(&dir).map_err(|error| {
            format!(
                "failed to create temp wallet dir `{}`: {error}",
                dir.display()
            )
        })?;
        Ok(Self(dir))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempWalletDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
