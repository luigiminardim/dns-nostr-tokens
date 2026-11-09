use crate::application::create_wallet::{CreateWallet, CreateWalletError};
use crate::application::get_receive_address::{GetReceiveAddress, GetReceiveAddressError};
use crate::application::import_wallet::{ImportWallet, ImportWalletError};
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
    /// Manage the wallet seed and addresses
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
}

#[derive(Debug)]
pub enum CliError {
    CreateWallet(CreateWalletError),
    ImportWallet(ImportWalletError),
    GetReceiveAddress(GetReceiveAddressError),
    ReadingStdin(io::ErrorKind),
}

/// Translates parsed commands into use-case calls and use-case results into output text.
pub struct CliController {
    create_wallet: CreateWallet,
    import_wallet: ImportWallet,
    get_receive_address: GetReceiveAddress,
}

impl CliController {
    pub fn new(
        create_wallet: CreateWallet,
        import_wallet: ImportWallet,
        get_receive_address: GetReceiveAddress,
    ) -> Self {
        Self {
            create_wallet,
            import_wallet,
            get_receive_address,
        }
    }

    pub fn execute(&self, command: Command, stdin: &mut dyn Read) -> Result<String, CliError> {
        match command {
            Command::Wallet { command } => self.execute_wallet_command(command, stdin),
        }
    }

    fn execute_wallet_command(
        &self,
        command: WalletCommand,
        stdin: &mut dyn Read,
    ) -> Result<String, CliError> {
        match command {
            WalletCommand::Create => self.create(),
            WalletCommand::Import { mnemonic } => self.import(mnemonic, stdin),
            WalletCommand::Address => self.address(),
        }
    }

    fn create(&self) -> Result<String, CliError> {
        let mnemonic = self.create_wallet.execute()?;
        Ok(format!(
            "Wallet created. Write down this seed phrase and keep it safe:\n{mnemonic}"
        ))
    }

    fn import(&self, words: Vec<String>, stdin: &mut dyn Read) -> Result<String, CliError> {
        let phrase = if words.is_empty() {
            Self::read_all(stdin)?
        } else {
            words.join(" ")
        };
        self.import_wallet.execute(&phrase)?;
        Ok("Wallet imported.".to_string())
    }

    fn address(&self) -> Result<String, CliError> {
        let info = self.get_receive_address.execute()?;
        Ok(info.address.to_string())
    }

    fn read_all(stdin: &mut dyn Read) -> Result<String, CliError> {
        let mut text = String::new();
        stdin
            .read_to_string(&mut text)
            .map_err(|error| CliError::ReadingStdin(error.kind()))?;
        Ok(text)
    }
}

impl From<CreateWalletError> for CliError {
    fn from(error: CreateWalletError) -> Self {
        Self::CreateWallet(error)
    }
}

impl From<ImportWalletError> for CliError {
    fn from(error: ImportWalletError) -> Self {
        Self::ImportWallet(error)
    }
}

impl From<GetReceiveAddressError> for CliError {
    fn from(error: GetReceiveAddressError) -> Self {
        Self::GetReceiveAddress(error)
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CreateWallet(error) => write!(formatter, "{error}"),
            Self::ImportWallet(error) => write!(formatter, "{error}"),
            Self::GetReceiveAddress(error) => write!(formatter, "{error}"),
            Self::ReadingStdin(kind) => write!(formatter, "could not read stdin: {kind}"),
        }
    }
}

impl std::error::Error for CliError {}

#[cfg(test)]
mod test_cli {
    use super::*;
    use crate::adapters::sqlite_name_token_wallet_repository::SqliteNameTokenWalletRepository;
    use crate::domain::name_token_wallet_repository::NameTokenWalletRepository;
    use bitcoin::Network;
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

    fn controller(repository: Arc<dyn NameTokenWalletRepository>) -> CliController {
        CliController::new(
            CreateWallet::new(Network::Regtest, Arc::clone(&repository)),
            ImportWallet::new(Network::Regtest, Arc::clone(&repository)),
            GetReceiveAddress::new(repository),
        )
    }

    fn run(controller: &CliController, command: Command, stdin: &str) -> Result<String, CliError> {
        controller.execute(command, &mut stdin.as_bytes())
    }

    #[test]
    fn parses_create() {
        let expected = wallet_command(WalletCommand::Create);
        assert_eq!(parse(&["wallet", "create"]).unwrap(), expected);
    }

    #[test]
    fn parses_import_with_words() {
        let expected = wallet_command(WalletCommand::Import {
            mnemonic: vec!["abandon".into(), "about".into()],
        });
        assert_eq!(
            parse(&["wallet", "import", "abandon", "about"]).unwrap(),
            expected
        );
    }

    #[test]
    fn parses_import_without_words() {
        let expected = wallet_command(WalletCommand::Import { mnemonic: vec![] });
        assert_eq!(parse(&["wallet", "import"]).unwrap(), expected);
    }

    #[test]
    fn parses_address() {
        let expected = wallet_command(WalletCommand::Address);
        assert_eq!(parse(&["wallet", "address"]).unwrap(), expected);
    }

    #[test]
    fn rejects_unknown_subcommand_or_flag() {
        assert!(parse(&["wallet", "explode"]).is_err());
        assert!(parse(&["wallet", "address", "--index", "0"]).is_err());
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
            Err(CliError::CreateWallet(
                CreateWalletError::WalletAlreadyExists
            ))
        ));
    }

    #[test]
    fn import_from_words_then_address() {
        let repository = in_memory_repository();
        let ctrl = controller(repository);
        let words = MNEMONIC.split_whitespace().map(String::from).collect();
        run(
            &ctrl,
            wallet_command(WalletCommand::Import { mnemonic: words }),
            "",
        )
        .unwrap();
        let address = run(&ctrl, wallet_command(WalletCommand::Address), "").unwrap();
        assert_eq!(address, REGTEST_EXTERNAL_0);
    }

    #[test]
    fn address_auto_advances_index_across_calls() {
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
            Err(CliError::GetReceiveAddress(
                GetReceiveAddressError::WalletNotFound
            ))
        ));
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
