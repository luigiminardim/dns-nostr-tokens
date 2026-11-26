use clap::Parser;
use dns_nostr_wallet::adapters::cli_controller::{Cli, CliController};
use dns_nostr_wallet::adapters::environment_config::EnvironmentConfig;
use dns_nostr_wallet::adapters::rpc_blockchain_adapter::RpcBlockchainAdapter;
use dns_nostr_wallet::adapters::sqlite_name_token_wallet_repository::SqliteNameTokenWalletRepository;
use dns_nostr_wallet::application::create_wallet_usecase::CreateWalletUsecase;
use dns_nostr_wallet::application::get_balance_usecase::GetBalanceUsecase;
use dns_nostr_wallet::application::get_receive_address_usecase::GetReceiveAddressUsecase;
use dns_nostr_wallet::application::import_wallet_usecase::ImportWalletUsecase;
use dns_nostr_wallet::application::send_bitcoin_usecase::SendBitcoinUsecase;
use dns_nostr_wallet::domain::blockchain_port::BlockchainPort;
use dns_nostr_wallet::domain::name_token_wallet_repository::NameTokenWalletRepository;
use dns_nostr_wallet::domain::sync_service::SyncService;
use std::process::ExitCode;
use std::sync::Arc;

fn main() -> ExitCode {
    let cli = Cli::parse();
    let config = match EnvironmentConfig::from_environment() {
        Ok(config) => config,
        Err(error) => return fail(error),
    };

    let repository: Arc<dyn NameTokenWalletRepository> =
        Arc::new(SqliteNameTokenWalletRepository::open(config.db_path()));
    let blockchain: Arc<dyn BlockchainPort> = Arc::new(RpcBlockchainAdapter::new(
        &config.bitcoin_rpc_url,
        &config.bitcoin_rpc_user,
        &config.bitcoin_rpc_password,
    ));
    let sync_service = Arc::new(SyncService);

    let controller = CliController::new(
        CreateWalletUsecase::new(
            config.network,
            Arc::clone(&repository),
            Arc::clone(&blockchain),
            Arc::clone(&sync_service),
        ),
        ImportWalletUsecase::new(
            config.network,
            Arc::clone(&repository),
            Arc::clone(&blockchain),
            Arc::clone(&sync_service),
        ),
        GetReceiveAddressUsecase::new(
            config.network,
            Arc::clone(&repository),
            Arc::clone(&blockchain),
            Arc::clone(&sync_service),
        ),
        GetBalanceUsecase::new(
            config.network,
            Arc::clone(&repository),
            Arc::clone(&blockchain),
            Arc::clone(&sync_service),
        ),
        SendBitcoinUsecase::new(config.network, repository, blockchain, sync_service),
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
