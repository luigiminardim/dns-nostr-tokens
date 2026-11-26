use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

pub struct JustCommand {
    workspace_root: PathBuf,
    rpc_user: String,
    rpc_password: String,
}

impl JustCommand {
    pub fn new(rpc_user: &str, rpc_password: &str) -> Result<Self, String> {
        let workspace_root = Self::workspace_root()?;
        Ok(Self {
            workspace_root,
            rpc_user: rpc_user.to_string(),
            rpc_password: rpc_password.to_string(),
        })
    }

    pub fn run(&self, args: &[&str]) -> Result<String, String> {
        let output = Command::new("just")
            .args(args)
            .current_dir(&self.workspace_root)
            .env("DNS_NOSTR_BITCOIN_RPC_USER", &self.rpc_user)
            .env("DNS_NOSTR_BITCOIN_RPC_PASSWORD", &self.rpc_password)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|error| format!("failed to execute `just {}`: {error}", args.join(" ")))?;

        if !output.status.success() {
            return Err(Self::format_command_error(
                args,
                output.status.code(),
                &String::from_utf8_lossy(&output.stderr),
            ));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    fn workspace_root() -> Result<PathBuf, String> {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| "unable to resolve workspace root directory".to_string())
    }

    fn format_command_error(args: &[&str], exit_code: Option<i32>, stderr: &str) -> String {
        let code = exit_code
            .map(|c| c.to_string())
            .unwrap_or_else(|| "signal".to_string());
        let trimmed_stderr = stderr.trim();
        if trimmed_stderr.is_empty() {
            format!("`just {}` failed with exit status {code}", args.join(" "))
        } else {
            format!(
                "`just {}` failed with exit status {code}: {trimmed_stderr}",
                args.join(" ")
            )
        }
    }
}

#[cfg(test)]
mod test_just_command {
    use super::*;

    #[test]
    fn workspace_root_contains_justfile() {
        let root = JustCommand::workspace_root().expect("workspace root should resolve");
        assert!(
            root.join("justfile").is_file(),
            "expected justfile at {}",
            root.display()
        );
    }

    #[test]
    fn format_command_error_includes_args_code_and_stderr() {
        let with_stderr = JustCommand::format_command_error(
            &["miner-fund", "bcrt1q...", "1.0"],
            Some(1),
            "boom\n",
        );
        assert_eq!(
            with_stderr,
            "`just miner-fund bcrt1q... 1.0` failed with exit status 1: boom"
        );

        let without_stderr = JustCommand::format_command_error(&["miner-balance"], None, "   ");
        assert_eq!(
            without_stderr,
            "`just miner-balance` failed with exit status signal"
        );
    }
}
