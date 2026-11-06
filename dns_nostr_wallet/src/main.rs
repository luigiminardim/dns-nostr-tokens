use clap::Parser;
use dns_nostr_wallet::adapters::cli::{Cli, CliController};
use dns_nostr_wallet::adapters::environment_config::EnvironmentConfig;
use dns_nostr_wallet::application::create_wallet::CreateWallet;
use dns_nostr_wallet::application::get_receive_address::GetReceiveAddress;
use dns_nostr_wallet::application::import_wallet::ImportWallet;
use rusqlite::Connection;
use std::fs;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};

fn main() -> ExitCode {
    let cli = Cli::parse();
    let config = match EnvironmentConfig::from_environment() {
        Ok(config) => config,
        Err(error) => return fail(error),
    };

    if let Err(error) = fs::create_dir_all(&config.wallet_dir) {
        return fail(format!(
            "could not create wallet directory `{}`: {error}",
            config.wallet_dir.display()
        ));
    }

    let db_path = config.db_path();
    let connection = match Connection::open(&db_path) {
        Ok(conn) => Arc::new(Mutex::new(conn)),
        Err(error) => {
            return fail(format!(
                "could not open wallet database `{}`: {error}",
                db_path.display()
            ))
        }
    };

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = fs::metadata(&db_path) {
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o600);
            let _ = fs::set_permissions(&db_path, permissions);
        }
    }

    let controller = CliController::new(
        CreateWallet::new(config.network),
        ImportWallet::new(config.network),
        GetReceiveAddress::new(),
        connection,
    );

    match controller.execute(cli.command, &mut std::io::stdin()) {
        Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
        }
        Err(error) => fail(error),
    }
}

fn fail(error: impl std::fmt::Display) -> ExitCode {
    eprintln!("error: {error}");
    ExitCode::FAILURE
}
