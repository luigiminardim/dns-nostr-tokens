use crate::application::create_wallet::{CreateWallet, CreateWalletError};
use crate::application::get_receive_address::{GetReceiveAddress, GetReceiveAddressError};
use crate::application::import_wallet::{ImportWallet, ImportWalletError};
use bdk_wallet::rusqlite::Connection;
use clap::{Parser, Subcommand};
use std::fmt;
use std::io::{self, Read};
use std::sync::{Arc, Mutex};

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
    /// Print a receive address for the selected network
    Address {
        #[arg(long)]
        index: Option<u32>,
    },
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
    connection: Arc<Mutex<Connection>>,
}

impl CliController {
    pub fn new(
        create_wallet: CreateWallet,
        import_wallet: ImportWallet,
        get_receive_address: GetReceiveAddress,
        connection: Arc<Mutex<Connection>>,
    ) -> Self {
        Self {
            create_wallet,
            import_wallet,
            get_receive_address,
            connection,
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
        let mut conn = self.connection.lock().unwrap();
        match command {
            WalletCommand::Create => self.create(&mut conn),
            WalletCommand::Import { mnemonic } => self.import(mnemonic, stdin, &mut conn),
            WalletCommand::Address { index } => self.address(index, &mut conn),
        }
    }

    fn create(&self, conn: &mut Connection) -> Result<String, CliError> {
        let mnemonic = self.create_wallet.execute(conn)?;
        Ok(format!(
            "Wallet created. Write down this seed phrase and keep it safe:\n{mnemonic}"
        ))
    }

    fn import(
        &self,
        words: Vec<String>,
        stdin: &mut dyn Read,
        conn: &mut Connection,
    ) -> Result<String, CliError> {
        let phrase = if words.is_empty() {
            read_all(stdin)?
        } else {
            words.join(" ")
        };
        self.import_wallet.execute(conn, &phrase)?;
        Ok("Wallet imported.".to_string())
    }

    fn address(&self, index: Option<u32>, conn: &mut Connection) -> Result<String, CliError> {
        let info = self.get_receive_address.execute(conn, index)?;
        Ok(info.address.to_string())
    }
}

fn read_all(stdin: &mut dyn Read) -> Result<String, CliError> {
    let mut text = String::new();
    stdin
        .read_to_string(&mut text)
        .map_err(|error| CliError::ReadingStdin(error.kind()))?;
    Ok(text)
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
    use crate::domain::dns_nostr_wallet::DnsNostrWallet;
    use bitcoin::Network;

    const MNEMONIC: &str = "abandon abandon abandon abandon abandon abandon \
                            abandon abandon abandon abandon abandon about";
    const REGTEST_EXTERNAL_0: &str = "bcrt1q6rz28mcfaxtmd6v789l9rrlrusdprr9pz3cppk";
    const REGTEST_EXTERNAL_1: &str = "bcrt1qd7spv5q28348xl4myc8zmh983w5jx32cs707jh";

    fn in_memory_connection() -> Arc<Mutex<Connection>> {
        Arc::new(Mutex::new(Connection::open_in_memory().unwrap()))
    }

    fn parse(arguments: &[&str]) -> Result<Command, clap::Error> {
        let program = ["dns_nostr_wallet"];
        Cli::try_parse_from(program.iter().chain(arguments)).map(|cli| cli.command)
    }

    fn wallet_command(command: WalletCommand) -> Command {
        Command::Wallet { command }
    }

    fn controller(connection: Arc<Mutex<Connection>>) -> CliController {
        CliController::new(
            CreateWallet::new(Network::Regtest),
            ImportWallet::new(Network::Regtest),
            GetReceiveAddress::new(),
            connection,
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
    fn parses_address_with_index() {
        let expected = wallet_command(WalletCommand::Address { index: Some(3) });
        assert_eq!(
            parse(&["wallet", "address", "--index", "3"]).unwrap(),
            expected
        );
    }

    #[test]
    fn address_index_defaults_to_none() {
        let expected = wallet_command(WalletCommand::Address { index: None });
        assert_eq!(parse(&["wallet", "address"]).unwrap(), expected);
    }

    #[test]
    fn rejects_unknown_subcommand() {
        assert!(parse(&["wallet", "explode"]).is_err());
        assert!(parse(&["nonsense"]).is_err());
    }

    #[test]
    fn create_prints_a_12_word_seed_phrase() {
        let connection = in_memory_connection();
        let output = run(
            &controller(connection.clone()),
            wallet_command(WalletCommand::Create),
            "",
        )
        .unwrap();
        let phrase = output.lines().last().unwrap();
        assert_eq!(phrase.split(' ').count(), 12);

        let mut conn = connection.lock().unwrap();
        let wallet = DnsNostrWallet::load(&mut conn).unwrap();
        assert!(wallet.is_some());
    }

    #[test]
    fn second_create_fails() {
        let connection = in_memory_connection();
        let ctrl = controller(connection);
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
        let connection = in_memory_connection();
        let ctrl = controller(connection);
        let words = MNEMONIC.split_whitespace().map(String::from).collect();
        run(
            &ctrl,
            wallet_command(WalletCommand::Import { mnemonic: words }),
            "",
        )
        .unwrap();
        let address = run(
            &ctrl,
            wallet_command(WalletCommand::Address { index: None }),
            "",
        )
        .unwrap();
        assert_eq!(address, REGTEST_EXTERNAL_0);
    }

    #[test]
    fn import_from_words_then_address_peeks_index() {
        let connection = in_memory_connection();
        let ctrl = controller(connection);
        let words = MNEMONIC.split_whitespace().map(String::from).collect();
        run(
            &ctrl,
            wallet_command(WalletCommand::Import { mnemonic: words }),
            "",
        )
        .unwrap();
        let address = run(
            &ctrl,
            wallet_command(WalletCommand::Address { index: Some(1) }),
            "",
        )
        .unwrap();
        assert_eq!(address, REGTEST_EXTERNAL_1);
    }

    #[test]
    fn address_auto_advances_index_across_calls() {
        let connection = in_memory_connection();
        let ctrl = controller(connection);
        let words = MNEMONIC.split_whitespace().map(String::from).collect();
        run(
            &ctrl,
            wallet_command(WalletCommand::Import { mnemonic: words }),
            "",
        )
        .unwrap();

        let addr0 = run(
            &ctrl,
            wallet_command(WalletCommand::Address { index: None }),
            "",
        )
        .unwrap();
        assert_eq!(addr0, REGTEST_EXTERNAL_0);

        let addr1 = run(
            &ctrl,
            wallet_command(WalletCommand::Address { index: None }),
            "",
        )
        .unwrap();
        assert_eq!(addr1, REGTEST_EXTERNAL_1);

        // Peeking at index 0 does not disturb the next index
        let peek0 = run(
            &ctrl,
            wallet_command(WalletCommand::Address { index: Some(0) }),
            "",
        )
        .unwrap();
        assert_eq!(peek0, REGTEST_EXTERNAL_0);

        let addr2 = run(
            &ctrl,
            wallet_command(WalletCommand::Address { index: None }),
            "",
        )
        .unwrap();
        assert_ne!(addr2, addr0);
        assert_ne!(addr2, addr1);
    }

    #[test]
    fn import_reads_stdin_when_no_words_are_given() {
        let connection = in_memory_connection();
        let ctrl = controller(connection.clone());
        let import = wallet_command(WalletCommand::Import { mnemonic: vec![] });
        run(&ctrl, import, MNEMONIC).unwrap();

        let mut conn = connection.lock().unwrap();
        assert!(DnsNostrWallet::load(&mut conn).unwrap().is_some());
    }

    #[test]
    fn invalid_import_fails_without_echoing_the_phrase() {
        let connection = in_memory_connection();
        let import = wallet_command(WalletCommand::Import {
            mnemonic: vec!["zebra".into(), "zebra".into()],
        });
        let error = run(&controller(connection.clone()), import, "").unwrap_err();
        assert!(!error.to_string().contains("zebra"), "{error}");

        let mut conn = connection.lock().unwrap();
        assert!(DnsNostrWallet::load(&mut conn).unwrap().is_none());
    }

    #[test]
    fn address_fails_without_wallet() {
        let connection = in_memory_connection();
        let result = run(
            &controller(connection),
            wallet_command(WalletCommand::Address { index: None }),
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
        let connection = in_memory_connection();
        let ctrl = controller(connection);
        let import = wallet_command(WalletCommand::Import {
            mnemonic: MNEMONIC.split(' ').map(String::from).collect(),
        });
        let imported = run(&ctrl, import, "").unwrap();
        let address = run(
            &ctrl,
            wallet_command(WalletCommand::Address { index: None }),
            "",
        )
        .unwrap();
        for output in [imported, address] {
            assert!(!output.contains("abandon"), "{output}");
        }
    }
}
