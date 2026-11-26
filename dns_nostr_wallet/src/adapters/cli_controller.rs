use crate::application::create_wallet_usecase::{CreateWalletUsecase, CreateWalletUsecaseError};
use crate::application::get_balance_usecase::{GetBalanceUsecase, GetBalanceUsecaseError};
use crate::application::get_receive_address_usecase::{
    GetReceiveAddressUsecase, GetReceiveAddressUsecaseError,
};
use crate::application::import_wallet_usecase::{ImportWalletUsecase, ImportWalletUsecaseError};
use crate::application::send_bitcoin_usecase::{SendBitcoinUsecase, SendBitcoinUsecaseError};
use bitcoin::Denomination;
use clap::{Parser, Subcommand};
use std::fmt;
use std::io::{self, Read};

/// `dns_nostr_wallet` command line.
#[derive(Debug, Parser)]
#[command(name = "dns_nostr_wallet", about = "DNS-Nostr Wallet")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Clone, PartialEq, Eq, Subcommand)]
pub enum Command {
    /// Manage the wallet seed, addresses, balance, and Bitcoin transfers
    Wallet {
        #[command(subcommand)]
        command: WalletCommand,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Subcommand)]
pub enum WalletCommand {
    /// Generate a new 12-word BIP39 seed phrase and store it
    Create,
    /// Import an existing BIP39 seed phrase (words as arguments, or from stdin when omitted)
    Import { mnemonic: Vec<String> },
    /// Print the next receive address for the selected network
    Address,
    /// Print the spendable wallet balance in BTC
    Balance,
    /// Send Bitcoin to a recipient address
    Send {
        /// Recipient Bitcoin address
        address: String,
        /// Amount to send in BTC (decimal, up to 8 decimal places)
        amount_btc: String,
    },
}

#[derive(Debug)]
pub enum CliControllerError {
    CreateWallet(CreateWalletUsecaseError),
    ImportWallet(ImportWalletUsecaseError),
    GetReceiveAddress(GetReceiveAddressUsecaseError),
    GetBalance(GetBalanceUsecaseError),
    SendBitcoin(SendBitcoinUsecaseError),
    ReadingStdin(io::ErrorKind),
}

/// Translates parsed commands into use-case calls and use-case results into output text.
pub struct CliController {
    create_wallet: CreateWalletUsecase,
    import_wallet: ImportWalletUsecase,
    get_receive_address: GetReceiveAddressUsecase,
    get_balance: GetBalanceUsecase,
    send_bitcoin: SendBitcoinUsecase,
}

impl CliController {
    pub fn new(
        create_wallet: CreateWalletUsecase,
        import_wallet: ImportWalletUsecase,
        get_receive_address: GetReceiveAddressUsecase,
        get_balance: GetBalanceUsecase,
        send_bitcoin: SendBitcoinUsecase,
    ) -> Self {
        Self {
            create_wallet,
            import_wallet,
            get_receive_address,
            get_balance,
            send_bitcoin,
        }
    }

    pub fn execute(
        &self,
        command: Command,
        stdin: &mut dyn Read,
    ) -> Result<String, CliControllerError> {
        match command {
            Command::Wallet { command } => self.execute_wallet_command(command, stdin),
        }
    }

    fn execute_wallet_command(
        &self,
        command: WalletCommand,
        stdin: &mut dyn Read,
    ) -> Result<String, CliControllerError> {
        match command {
            WalletCommand::Create => self.create(),
            WalletCommand::Import { mnemonic } => self.import(mnemonic, stdin),
            WalletCommand::Address => self.address(),
            WalletCommand::Balance => self.balance(),
            WalletCommand::Send {
                address,
                amount_btc,
            } => self.send(&address, &amount_btc),
        }
    }

    fn create(&self) -> Result<String, CliControllerError> {
        let mnemonic = self.create_wallet.execute()?;
        Ok(format!(
            "Wallet created. Write down this seed phrase and keep it safe:\n{mnemonic}"
        ))
    }

    fn import(
        &self,
        words: Vec<String>,
        stdin: &mut dyn Read,
    ) -> Result<String, CliControllerError> {
        let phrase = if words.is_empty() {
            Self::read_all(stdin)?
        } else {
            words.join(" ")
        };
        self.import_wallet.execute(&phrase)?;
        Ok("Wallet imported.".to_string())
    }

    fn address(&self) -> Result<String, CliControllerError> {
        let info = self.get_receive_address.execute()?;
        Ok(info.address.to_string())
    }

    fn balance(&self) -> Result<String, CliControllerError> {
        let amount = self.get_balance.execute()?;
        Ok(amount.to_string_in(Denomination::Bitcoin))
    }

    fn send(&self, address: &str, amount_btc: &str) -> Result<String, CliControllerError> {
        let txid = self.send_bitcoin.execute(address, amount_btc)?;
        Ok(txid.to_string())
    }

    fn read_all(stdin: &mut dyn Read) -> Result<String, CliControllerError> {
        let mut text = String::new();
        stdin
            .read_to_string(&mut text)
            .map_err(|error| CliControllerError::ReadingStdin(error.kind()))?;
        Ok(text)
    }
}

impl From<CreateWalletUsecaseError> for CliControllerError {
    fn from(error: CreateWalletUsecaseError) -> Self {
        Self::CreateWallet(error)
    }
}

impl From<ImportWalletUsecaseError> for CliControllerError {
    fn from(error: ImportWalletUsecaseError) -> Self {
        Self::ImportWallet(error)
    }
}

impl From<GetReceiveAddressUsecaseError> for CliControllerError {
    fn from(error: GetReceiveAddressUsecaseError) -> Self {
        Self::GetReceiveAddress(error)
    }
}

impl From<GetBalanceUsecaseError> for CliControllerError {
    fn from(error: GetBalanceUsecaseError) -> Self {
        Self::GetBalance(error)
    }
}

impl From<SendBitcoinUsecaseError> for CliControllerError {
    fn from(error: SendBitcoinUsecaseError) -> Self {
        Self::SendBitcoin(error)
    }
}

impl fmt::Display for CliControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CreateWallet(error) => write!(formatter, "{error}"),
            Self::ImportWallet(error) => write!(formatter, "{error}"),
            Self::GetReceiveAddress(error) => write!(formatter, "{error}"),
            Self::GetBalance(error) => write!(formatter, "{error}"),
            Self::SendBitcoin(error) => write!(formatter, "{error}"),
            Self::ReadingStdin(kind) => write!(formatter, "could not read stdin: {kind}"),
        }
    }
}

impl std::error::Error for CliControllerError {}

#[cfg(test)]
mod test_cli_controller {
    use super::*;
    use crate::adapters::sqlite_name_token_wallet_repository::SqliteNameTokenWalletRepository;
    use crate::domain::blockchain_port::{BlockchainError, BlockchainPort, FakeBlockchain};
    use crate::domain::name_token_wallet_repository::NameTokenWalletRepository;
    use crate::domain::sync_service::SyncService;
    use bitcoin::absolute::LockTime;
    use bitcoin::block::{Header, Version as BlockVersion};
    use bitcoin::hashes::Hash;
    use bitcoin::transaction::Version as TxVersion;
    use bitcoin::{
        Amount, Block, CompactTarget, Network, OutPoint, ScriptBuf, Sequence, Transaction, TxIn,
        TxMerkleNode, TxOut, Txid, Witness,
    };
    use std::sync::Arc;

    const MNEMONIC: &str = "abandon abandon abandon abandon abandon abandon \
                            abandon abandon abandon abandon abandon about";
    const REGTEST_EXTERNAL_0: &str = "bcrt1q6rz28mcfaxtmd6v789l9rrlrusdprr9pz3cppk";
    const REGTEST_EXTERNAL_1: &str = "bcrt1qd7spv5q28348xl4myc8zmh983w5jx32cs707jh";

    fn in_memory_repository() -> Arc<dyn NameTokenWalletRepository> {
        Arc::new(SqliteNameTokenWalletRepository::in_memory().unwrap())
    }

    fn parse(arguments: &[&str]) -> Result<Command, clap::Error> {
        let program = ["dns_nostr_wallet"];
        Cli::try_parse_from(program.iter().chain(arguments)).map(|cli| cli.command)
    }

    fn wallet_command(command: WalletCommand) -> Command {
        Command::Wallet { command }
    }

    fn controller_for(
        network: Network,
        repository: Arc<dyn NameTokenWalletRepository>,
        blockchain: Arc<dyn BlockchainPort>,
    ) -> CliController {
        let sync_service = Arc::new(SyncService);
        CliController::new(
            CreateWalletUsecase::new(
                network,
                Arc::clone(&repository),
                Arc::clone(&blockchain),
                Arc::clone(&sync_service),
            ),
            ImportWalletUsecase::new(
                network,
                Arc::clone(&repository),
                Arc::clone(&blockchain),
                Arc::clone(&sync_service),
            ),
            GetReceiveAddressUsecase::new(
                network,
                Arc::clone(&repository),
                Arc::clone(&blockchain),
                Arc::clone(&sync_service),
            ),
            GetBalanceUsecase::new(
                network,
                Arc::clone(&repository),
                Arc::clone(&blockchain),
                Arc::clone(&sync_service),
            ),
            SendBitcoinUsecase::new(network, repository, blockchain, sync_service),
        )
    }

    fn controller(repository: Arc<dyn NameTokenWalletRepository>) -> CliController {
        controller_for(Network::Regtest, repository, Arc::new(FakeBlockchain::new()))
    }

    fn run(
        controller: &CliController,
        command: Command,
        stdin: &str,
    ) -> Result<String, CliControllerError> {
        controller.execute(command, &mut stdin.as_bytes())
    }

    #[test]
    fn parses_all_wallet_subcommands() {
        assert_eq!(
            parse(&["wallet", "create"]).unwrap(),
            wallet_command(WalletCommand::Create)
        );
        assert_eq!(
            parse(&["wallet", "import", "abandon", "about"]).unwrap(),
            wallet_command(WalletCommand::Import {
                mnemonic: vec!["abandon".into(), "about".into()],
            })
        );
        assert_eq!(
            parse(&["wallet", "import"]).unwrap(),
            wallet_command(WalletCommand::Import { mnemonic: vec![] })
        );
        assert_eq!(
            parse(&["wallet", "address"]).unwrap(),
            wallet_command(WalletCommand::Address)
        );
        assert_eq!(
            parse(&["wallet", "balance"]).unwrap(),
            wallet_command(WalletCommand::Balance)
        );
        assert_eq!(
            parse(&["wallet", "send", REGTEST_EXTERNAL_0, "0.25"]).unwrap(),
            wallet_command(WalletCommand::Send {
                address: REGTEST_EXTERNAL_0.into(),
                amount_btc: "0.25".into(),
            })
        );
    }

    #[test]
    fn rejects_unknown_subcommand_or_flag() {
        assert!(parse(&["wallet", "explode"]).is_err());
        assert!(parse(&["wallet", "address", "--index", "0"]).is_err());
        assert!(parse(&["wallet", "send", REGTEST_EXTERNAL_0, "0.25", "--yes"]).is_err());
        assert!(parse(&["wallet", "send", REGTEST_EXTERNAL_0, "0.25", "--fee-rate", "2"]).is_err());
        assert!(parse(&["nonsense"]).is_err());
    }

    #[test]
    fn create_prints_a_12_word_seed_phrase() {
        let repository = in_memory_repository();
        let output = run(
            &controller(Arc::clone(&repository)),
            wallet_command(WalletCommand::Create),
            "",
        )
        .unwrap();
        let phrase = output.lines().last().unwrap();
        assert_eq!(phrase.split(' ').count(), 12);

        let wallet = repository.load().unwrap();
        assert!(wallet.is_some());
    }

    #[test]
    fn second_create_fails() {
        let repository = in_memory_repository();
        let ctrl = controller(repository);
        run(&ctrl, wallet_command(WalletCommand::Create), "").unwrap();
        let result = run(&ctrl, wallet_command(WalletCommand::Create), "");
        assert!(matches!(
            result,
            Err(CliControllerError::CreateWallet(
                CreateWalletUsecaseError::WalletAlreadyExists
            ))
        ));
    }

    #[test]
    fn import_from_words_then_address_auto_advances() {
        let repository = in_memory_repository();
        let ctrl = controller(repository);
        let words = MNEMONIC.split_whitespace().map(String::from).collect();
        run(
            &ctrl,
            wallet_command(WalletCommand::Import { mnemonic: words }),
            "",
        )
        .unwrap();

        let addr0 = run(&ctrl, wallet_command(WalletCommand::Address), "").unwrap();
        assert_eq!(addr0, REGTEST_EXTERNAL_0);

        let addr1 = run(&ctrl, wallet_command(WalletCommand::Address), "").unwrap();
        assert_eq!(addr1, REGTEST_EXTERNAL_1);

        let addr2 = run(&ctrl, wallet_command(WalletCommand::Address), "").unwrap();
        assert_ne!(addr2, addr0);
        assert_ne!(addr2, addr1);
    }

    #[test]
    fn address_uses_testnet_and_mainnet_prefixes() {
        for (network, prefix) in [(Network::Testnet, "tb1q"), (Network::Bitcoin, "bc1q")] {
            let repo = in_memory_repository();
            let ctrl = controller_for(network, repo, Arc::new(FakeBlockchain::new()));
            let import = wallet_command(WalletCommand::Import { mnemonic: vec![] });
            run(&ctrl, import, MNEMONIC).unwrap();
            let address = run(&ctrl, wallet_command(WalletCommand::Address), "").unwrap();
            assert!(address.starts_with(prefix), "got {address}");
        }
    }

    #[test]
    fn import_reads_stdin_when_no_words_are_given() {
        let repository = in_memory_repository();
        let ctrl = controller(Arc::clone(&repository));
        let import = wallet_command(WalletCommand::Import { mnemonic: vec![] });
        run(&ctrl, import, MNEMONIC).unwrap();

        assert!(repository.load().unwrap().is_some());
    }

    #[test]
    fn invalid_import_fails_without_echoing_the_phrase() {
        let repository = in_memory_repository();
        let import = wallet_command(WalletCommand::Import {
            mnemonic: vec!["zebra".into(), "zebra".into()],
        });
        let error = run(&controller(Arc::clone(&repository)), import, "").unwrap_err();
        assert!(!error.to_string().contains("zebra"), "{error}");

        assert!(repository.load().unwrap().is_none());
    }

    #[test]
    fn address_fails_without_wallet() {
        let repository = in_memory_repository();
        let result = run(
            &controller(repository),
            wallet_command(WalletCommand::Address),
            "",
        );
        assert!(matches!(
            result,
            Err(CliControllerError::GetReceiveAddress(
                GetReceiveAddressUsecaseError::WalletNotFound
            ))
        ));
    }

    #[test]
    fn balance_and_send_format_bare_btc_and_txid() {
        let repository = in_memory_repository();
        let blockchain = Arc::new(FakeBlockchain::new());
        let ctrl = controller_for(
            Network::Regtest,
            Arc::clone(&repository),
            Arc::clone(&blockchain) as Arc<dyn BlockchainPort>,
        );

        run(
            &ctrl,
            wallet_command(WalletCommand::Import { mnemonic: vec![] }),
            MNEMONIC,
        )
        .unwrap();
        assert_eq!(
            run(&ctrl, wallet_command(WalletCommand::Balance), "").unwrap(),
            "0"
        );

        // Fund wallet during next sync.
        blockchain.set_sync_hook(|wallet| {
            let addr = wallet.reveal_next_bitcoin_address().address;
            let genesis = wallet.latest_checkpoint().block_id();
            let fund_tx = Transaction {
                version: TxVersion::TWO,
                lock_time: LockTime::ZERO,
                input: vec![TxIn {
                    previous_output: OutPoint {
                        txid: Txid::from_byte_array([0x77; 32]),
                        vout: 0,
                    },
                    script_sig: ScriptBuf::new(),
                    sequence: Sequence::MAX,
                    witness: Witness::new(),
                }],
                output: vec![TxOut {
                    value: Amount::from_btc(1.5).unwrap(),
                    script_pubkey: addr.script_pubkey(),
                }],
            };
            let block1 = Block {
                header: Header {
                    version: BlockVersion::TWO,
                    prev_blockhash: genesis.hash,
                    merkle_root: TxMerkleNode::all_zeros(),
                    time: 1_700_000_001,
                    bits: CompactTarget::from_consensus(0x207f_ffff),
                    nonce: 1,
                },
                txdata: vec![fund_tx],
            };
            wallet
                .apply_block_connected_to(&block1, 1, genesis)
                .map_err(|error| BlockchainError::ChainUpdate(error.to_string()))
        });
        assert_eq!(
            run(&ctrl, wallet_command(WalletCommand::Balance), "").unwrap(),
            "1.5"
        );

        // Clear sync hook and send 0.25 BTC.
        blockchain.set_sync_hook(|_| Ok(()));
        let txid = run(
            &ctrl,
            wallet_command(WalletCommand::Send {
                address: REGTEST_EXTERNAL_1.into(),
                amount_btc: "0.25".into(),
            }),
            "",
        )
        .unwrap();
        assert_eq!(txid.len(), 64);
        assert!(txid.chars().all(|c| c.is_ascii_hexdigit()), "{txid}");
    }

    #[test]
    fn address_output_never_contains_seed_words() {
        let repository = in_memory_repository();
        let ctrl = controller(repository);
        let import = wallet_command(WalletCommand::Import {
            mnemonic: MNEMONIC.split(' ').map(String::from).collect(),
        });
        let imported = run(&ctrl, import, "").unwrap();
        let address = run(&ctrl, wallet_command(WalletCommand::Address), "").unwrap();
        for output in [imported, address] {
            assert!(!output.contains("abandon"), "{output}");
        }
    }
}
