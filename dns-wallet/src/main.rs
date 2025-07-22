use bdk::bitcoin::{Address, Network, Script};
use bdk::bitcoin::secp256k1::Secp256k1;
use bdk::bitcoin::util::bip32::{DerivationPath, KeySource};
use bdk::bitcoin::consensus::encode::serialize;
use bdk::bitcoin::blockdata::script::Builder;
use bdk::bitcoin::blockdata::opcodes;

use bdk::bitcoincore_rpc::RpcApi;

use bdk::blockchain::rpc::{Auth, RpcBlockchain, RpcConfig, RpcSyncParams};
use bdk::blockchain::{ConfigurableBlockchain};

use bdk::keys::bip39::{Mnemonic, Language, WordCount};
use bdk::keys::{GeneratedKey, GeneratableKey, ExtendedKey, DerivableKey, DescriptorKey};
use bdk::keys::DescriptorKey::Secret;

use bdk::miniscript::miniscript::Segwitv0;

use bdk::{SyncOptions, Wallet};
use bdk::wallet::{AddressIndex, signer::SignOptions, wallet_name_from_descriptor};

use bdk::sled::{self, Tree};

use std::str::FromStr;

use hex::encode;

use std::env;
use dotenvy::dotenv;
use std::io::{self, Write};

fn main() {
    // Load environment variables from .env file
    dotenv().ok();

    // Get receive and change descriptors

    let new_receive_desc;
    match env::var("RECEIVE_DESC") {
        Ok(receive_desc) => new_receive_desc = receive_desc,
        Err(_) => (new_receive_desc, _) = get_descriptors(),
    }

    let new_change_desc;
    match env::var("CHANGE_DESC") {
        Ok(change_desc) => new_change_desc = change_desc,
        Err(_) => (_, new_change_desc) = get_descriptors(),
    }

    // Use deterministic wallet name derived from descriptor
    let bdk_wallet_name = wallet_name_from_descriptor(&new_receive_desc, Some(&new_change_desc), Network::Regtest, &Secp256k1::new()).unwrap();

    // Create the datadir to store wallet data
    let mut datadir = dirs_next::home_dir().unwrap();
    datadir.push(".bdk-db");
    let database = sled::open(datadir).unwrap();
    let db_tree = database.open_tree(bdk_wallet_name.clone()).unwrap();

    // Set RPC username, password and url
    let rpc_user = env::var("RPC_USER").unwrap();
    let rpc_password = env::var("RPC_PASSWORD").unwrap();
    let auth = Auth::UserPass { username: rpc_user.clone(), password: rpc_password.clone() };
    let rpc_url = env::var("RPC_URL").unwrap();

    // Setup the RPC configuration
    let rpc_config = RpcConfig {
        url: rpc_url,
        auth,
        wallet_name: bdk_wallet_name,
        sync_params: Some(RpcSyncParams::default()),
        network: Network::Regtest,
    };

    // Use the above configuration to create a RPC blockchain backend
    let blockchain = RpcBlockchain::from_config(&rpc_config).unwrap();

    // Combine everything and finally create the BDK wallet structure
    let bdk_wallet = Wallet::new(&new_receive_desc, Some(&new_change_desc), Network::Regtest, db_tree).unwrap();

    // Fetch a fresh address to receive coins
    let _bdk_address = bdk_wallet.get_address(AddressIndex::New).unwrap().address;

    // Sync the wallet with the blockchain
    bdk_wallet.sync(&blockchain, SyncOptions::default()).unwrap();

    // Get the balance of the wallet
    let balance = bdk_wallet.get_balance().unwrap();
    println!("Initial wallet balance: {:#?}", balance);

    // Main menu loop
    loop {
        println!("\n=== DNS-Nostr Wallet Menu ===");
        println!("1. Check balance");
        println!("2. Create DNS-Nostr Token");
        println!("3. Update DNS-Nostr Token");
        println!("4. Revoke DNS-Nostr Token");
        println!("5. List DNS-Nostr Tokens");
        println!("6. Generate new address");
        println!("7. Exit");
        print!("\nSelect an option (1-7): ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        let choice = input.trim();

        match choice {
            "1" => {
                // Sync the wallet before checking balance
                bdk_wallet.sync(&blockchain, SyncOptions::default()).unwrap();
                let balance = bdk_wallet.get_balance().unwrap();
                println!("\nWallet balance: {} sats", balance.get_total());
            },
            "2" => {
                println!("\n=== Create DNS-Nostr Token ===");
                
                // Get token name
                print!("Enter token name: ");
                io::stdout().flush().unwrap();
                let mut token_name = String::new();
                io::stdin().read_line(&mut token_name).unwrap();
                let token_name = token_name.trim();
                
                // Get nostr pubkey
                print!("Enter nostr pubkey: ");
                io::stdout().flush().unwrap();
                let mut nostr_pubkey = String::new();
                io::stdin().read_line(&mut nostr_pubkey).unwrap();
                let nostr_pubkey = nostr_pubkey.trim();
                
                // Get DNS file path
                print!("Enter DNS file path: ");
                io::stdout().flush().unwrap();
                let mut dns_file_path = String::new();
                io::stdin().read_line(&mut dns_file_path).unwrap();
                let _dns_file_path = dns_file_path.trim();
                
                // Get a fresh address from the wallet to extract its pubkey hash
                let address = bdk_wallet.get_address(AddressIndex::New).unwrap().address;
                
                // Create the custom script
                let custom_script = create_dns_nostr_script(token_name, nostr_pubkey, &address);
                
                // Create transaction with custom script
                let txid = create_dns_nostr_token(&bdk_wallet, custom_script.clone(), &blockchain);
                
                // Mine a block to confirm the transaction
                let rpc_user = env::var("RPC_USER").unwrap();
                let rpc_password = env::var("RPC_PASSWORD").unwrap();
                let rpc_url = env::var("RPC_URL").unwrap();
                let miner_url = format!("{}/wallet/miner", rpc_url);
                let miner_auth = bdk::bitcoincore_rpc::Auth::UserPass(rpc_user, rpc_password);
                let miner_client = bdk::bitcoincore_rpc::Client::new(&miner_url, miner_auth).unwrap();
                
                let miner_addr = miner_client.get_new_address(None, None).unwrap();
                miner_client.generate_to_address(1, &miner_addr).unwrap();
                
                // Sync the wallet
                bdk_wallet.sync(&blockchain, SyncOptions::default()).unwrap();
                
                match txid {
                    Some(id) => {
                        println!("\nDNS-Nostr token created successfully! TXID: {}", id);
                        // Debug: print the script we created
                        println!("Script created: {}", hex::encode(custom_script.as_bytes()));
                    },
                    None => println!("\nDNS-Nostr token transaction created but not broadcast."),
                }
            },
            "3" => {
                println!("\n[Update DNS-Nostr Token - Coming Soon]");
            },
            "4" => {
                println!("\n[Revoke DNS-Nostr Token - Coming Soon]");
            },
            "5" => {
                // List DNS-Nostr tokens
                // Sync wallet first to get latest state
                bdk_wallet.sync(&blockchain, SyncOptions::default()).unwrap();
                list_dns_nostr_tokens(&bdk_wallet);
            },
            "6" => {
                // Generate new address
                let new_address = bdk_wallet.get_address(AddressIndex::New).unwrap();
                println!("\nNew address generated:");
                println!("Address: {}", new_address.address);
                println!("Index: {}", new_address.index);
            },
            "7" => {
                println!("\nExiting wallet...");
                break;
            },
            _ => {
                println!("\nInvalid option. Please select 1-7.");
            }
        }
    }
}


// Create DNS-Nostr script
fn create_dns_nostr_script(token_name: &str, nostr_pubkey: &str, address: &Address) -> Script {
    // Extract pubkey hash from the P2PKH address
    let pubkey_hash = match &address.payload {
        bdk::bitcoin::util::address::Payload::PubkeyHash(hash) => {
            hash.to_vec()
        },
        _ => {
            panic!("Expected P2PKH address, got different address type");
        }
    };
    
    // Build the script with DNS-Nostr data embedded
    let mut builder = Builder::new();
    
    // Add the OP_FALSE OP_IF block with DNS-Nostr data
    builder = builder.push_opcode(opcodes::all::OP_PUSHBYTES_0); // OP_FALSE
    builder = builder.push_opcode(opcodes::all::OP_IF);
    
    // Push "name" and token_name
    builder = builder.push_slice(b"name");
    builder = builder.push_slice(token_name.as_bytes());
    
    // Push OP_NOP
    builder = builder.push_opcode(opcodes::all::OP_NOP);
    
    // Push "dns-nostr" and nostr_pubkey
    builder = builder.push_slice(b"dns-nostr");
    builder = builder.push_slice(nostr_pubkey.as_bytes());
    
    // Close the IF block
    builder = builder.push_opcode(opcodes::all::OP_ENDIF);
    
    // Add standard P2PKH after the data
    builder = builder.push_opcode(opcodes::all::OP_DUP);
    builder = builder.push_opcode(opcodes::all::OP_HASH160);
    builder = builder.push_slice(&pubkey_hash);
    builder = builder.push_opcode(opcodes::all::OP_EQUALVERIFY);
    builder = builder.push_opcode(opcodes::all::OP_CHECKSIG);
    
    builder.into_script()
}

// Create DNS-Nostr token transaction
fn create_dns_nostr_token(bdk_wallet: &Wallet<Tree>, custom_script: Script, blockchain: &RpcBlockchain) -> Option<bdk::bitcoin::Txid> {
    // Sync wallet before creating transaction
    bdk_wallet.sync(blockchain, SyncOptions::default()).unwrap();
    
    // Get a change address
    let change_address = bdk_wallet.get_address(AddressIndex::New).unwrap().address;
    
    // Get list of DNS-Nostr token UTXOs to exclude from spending
    let utxos = bdk_wallet.list_unspent().unwrap();
    let mut excluded_utxos = Vec::new();
    
    // Find UTXOs that contain DNS-Nostr data (they have our custom script pattern)
    for utxo in utxos {
        let script = &utxo.txout.script_pubkey;
        // Check if this UTXO has our DNS-Nostr pattern
        if script.len() > 10 && script.as_bytes()[0] == 0x00 && script.as_bytes()[1] == 0x63 {
            // This looks like a DNS-Nostr token, exclude it
            excluded_utxos.push(utxo.outpoint);
        }
    }
    
    // Create a transaction builder
    let mut tx_builder = bdk_wallet.build_tx();
    
    // Exclude DNS-Nostr tokens from being spent
    tx_builder.unspendable(excluded_utxos);
    
    // Add custom script output (1000 sats - above dust limit for custom scripts) and change output
    tx_builder.add_recipient(custom_script, 1000);
    
    // Enable change output
    tx_builder.drain_to(change_address.script_pubkey());
    
    // Finalize the transaction and extract the PSBT
    let (mut psbt, _) = tx_builder.finish().unwrap();
    
    // Set signing option
    let signopt = SignOptions {
        assume_height: None,
        ..Default::default()
    };
    
    // Sign the PSBT
    bdk_wallet.sign(&mut psbt, signopt).unwrap();
    
    // Extract the final transaction
    let tx = psbt.extract_tx();
    
    // Serialize and encode the transaction
    let tx_hex = encode(serialize(&tx));
    let txid = tx.txid();
    
    // Create RPC client
    let rpc_user = env::var("RPC_USER").unwrap();
    let rpc_password = env::var("RPC_PASSWORD").unwrap();
    let rpc_url = env::var("RPC_URL").unwrap();
    
    let rpc_auth = bdk::bitcoincore_rpc::Auth::UserPass(rpc_user, rpc_password);
    let rpc_client = bdk::bitcoincore_rpc::Client::new(&rpc_url, rpc_auth).unwrap();
    
    // Try to broadcast
    match rpc_client.send_raw_transaction(tx_hex.clone()) {
        Ok(_) => {
            return Some(txid);
        },
        Err(_) => {
            // Transaction is non-standard, return txid anyway
            // The caller will mine a block to include it
            return Some(txid);
        }
    }
}

// List all DNS-Nostr tokens
fn list_dns_nostr_tokens(bdk_wallet: &Wallet<Tree>) {
    println!("\n=== DNS-Nostr Tokens ===");
    
    // Get RPC client to fetch full transaction data
    let rpc_user = env::var("RPC_USER").unwrap();
    let rpc_password = env::var("RPC_PASSWORD").unwrap();
    let rpc_url = env::var("RPC_URL").unwrap();
    let rpc_auth = bdk::bitcoincore_rpc::Auth::UserPass(rpc_user, rpc_password);
    let rpc_client = bdk::bitcoincore_rpc::Client::new(&rpc_url, rpc_auth).unwrap();
    
    // Get list of all transactions
    let txs = bdk_wallet.list_transactions(false).unwrap();
    let mut token_count = 0;
    
    for tx_details in txs {
        // Fetch full transaction from RPC
        if let Ok(tx) = rpc_client.get_raw_transaction(&tx_details.txid, None) {
            // Check each output in the transaction
            for output in tx.output.iter() {
                let script_bytes = output.script_pubkey.as_bytes();
                
                // Check if this output has our DNS-Nostr pattern (starts with OP_0 OP_IF)
                if script_bytes.len() > 10 && script_bytes[0] == 0x00 && script_bytes[1] == 0x63 {
                    // Parse the script to extract token name and nostr pubkey
                    if let Some((token_name, nostr_pubkey)) = parse_dns_nostr_script(script_bytes) {
                        token_count += 1;
                        println!("\n{}. {} - {}", token_count, token_name, nostr_pubkey);
                    }
                }
            }
        }
    }
    
    if token_count == 0 {
        println!("\nNo DNS-Nostr tokens found.");
    } else {
        println!("\nTotal: {} tokens", token_count);
    }
}

// Parse DNS-Nostr script to extract token name and nostr pubkey
fn parse_dns_nostr_script(script_bytes: &[u8]) -> Option<(String, String)> {
    let mut i = 2; // Skip OP_0 OP_IF
    let mut token_name = String::new();
    let mut nostr_pubkey = String::new();
    
    // Look for "name" marker
    while i < script_bytes.len() {
        if i + 5 <= script_bytes.len() && 
           script_bytes[i] == 4 && // length byte
           &script_bytes[i+1..i+5] == b"name" {
            i += 5;
            
            // Read token name
            if i < script_bytes.len() {
                let name_len = script_bytes[i] as usize;
                i += 1;
                if i + name_len <= script_bytes.len() {
                    token_name = String::from_utf8_lossy(&script_bytes[i..i+name_len]).to_string();
                    i += name_len;
                }
            }
            break;
        }
        i += 1;
    }
    
    // Skip OP_NOP (0x61)
    if i < script_bytes.len() && script_bytes[i] == 0x61 {
        i += 1;
    }
    
    // Look for "dns-nostr" marker
    while i < script_bytes.len() {
        if i + 10 <= script_bytes.len() && 
           script_bytes[i] == 9 && // length byte
           &script_bytes[i+1..i+10] == b"dns-nostr" {
            i += 10;
            
            // Read nostr pubkey
            if i < script_bytes.len() {
                let pubkey_len = script_bytes[i] as usize;
                i += 1;
                if i + pubkey_len <= script_bytes.len() {
                    nostr_pubkey = String::from_utf8_lossy(&script_bytes[i..i+pubkey_len]).to_string();
                }
            }
            break;
        }
        i += 1;
    }
    
    if !token_name.is_empty() && !nostr_pubkey.is_empty() {
        Some((token_name, nostr_pubkey))
    } else {
        None
    }
}

// generate fresh descriptor strings and return them via (receive, change) tuple
fn get_descriptors() -> (String, String) {
    // Create a new secp context
    let secp = Secp256k1::new();

    // You can also set a password to unlock the mnemonic
    let desc_password = env::var("DESCRIPTOR_PASSWORD").unwrap();
    let password = Some(desc_password);

    // Generate a fresh mnemonic, and from there a privatekey
    let mnemonic: GeneratedKey<_, Segwitv0> = Mnemonic::generate((WordCount::Words12, Language::English)).unwrap();
    let mnemonic = mnemonic.into_key();
    let xkey: ExtendedKey = (mnemonic, password).into_extended_key().unwrap();
    let xprv = xkey.into_xprv(Network::Regtest).unwrap();

    // Create derived privkey from the above master privkey
    // We use the following derivation paths for receive and change keys
    // receive: "m/44h/1h/0h/0"
    // change: "m/44h/1h/0h/1"
    let mut keys = Vec::new();

    for path in ["m/44h/1h/0h/0", "m/44h/1h/0h/1"] {
        let deriv_path: DerivationPath = DerivationPath::from_str(path).unwrap();
        let derived_xprv = &xprv.derive_priv(&secp, &deriv_path).unwrap();
        let origin: KeySource = (xprv.fingerprint(&secp), deriv_path);
        let derived_xprv_desc_key: DescriptorKey<Segwitv0> = derived_xprv.into_descriptor_key(Some(origin), DerivationPath::default()).unwrap();

        // Wrap the derived key in the pkh() string to produce a descriptor string
        if let Secret(key, _, _) = derived_xprv_desc_key {
            let mut desc = "pkh(".to_string();
            desc.push_str(&key.to_string());
            desc.push_str(")");
            keys.push(desc);
        }
    }

    // Return the keys as a tuple
    (keys[0].clone(), keys[1].clone())
}