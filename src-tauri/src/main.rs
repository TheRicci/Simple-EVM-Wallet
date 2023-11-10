// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use ethers::{
    prelude::*,
    core::rand,
    middleware::SignerMiddleware,
    core::k256::ecdsa::SigningKey,
    types::Address,
    core::rand::RngCore,
    signers::{Ledger, HDPath, coins_bip39::English, MnemonicBuilder,Wallet,LocalWallet}, etherscan::contract,
};
use log::{info, error,debug,LevelFilter};
use log4rs::append::console::ConsoleAppender;
use log4rs::config::{Appender, Root};
use log4rs::Config;

use secp256k1::*;

use tauri::{Manager, Window};

use std::{sync::Arc, collections::HashMap, fmt::format, vec,sync::Mutex};
use std::thread;

use std::rc::Rc;
use tokio::task;
use tokio::sync::mpsc::{Sender, Receiver};
use tokio::sync::mpsc;
use tokio::sync::RwLock;
use tokio::time::{Duration, interval};

use serde_json::json;

use ledger_transport_hid::{
    hidapi::{DeviceInfo, HidApi},
    TransportNativeHID,
};

enum Instruction {
    GetVersion,
    Sign,
    Commitment,
    GetPublicKey,
    GetPublicNonce,
    Exit,
}

impl Instruction {
    pub fn as_u8(&self) -> u8 {
        match self {
            Self::GetVersion => 0x01,
            Self::Sign => 0x02,
            Self::Commitment => 0x03,
            Self::GetPublicKey => 0x04,
            Self::GetPublicNonce => 0x05,
            Self::Exit => 0x06,
        }
    }
}

use ledger_transport::APDUCommand;

use serde::{Deserialize, Serialize};

use thiserror::Error;

use std::fs::{create_dir_all, File};
use std::io::prelude::*;
use std::fs;
use std::path::PathBuf;
use std::env;
use std::io::{self, BufRead};

use chrono::Local;
use chrono::format::strftime;

abigen!(
    IERC20,
    r#"[
        function balanceOf(address account) external view returns (uint256)
        function transfer(address recipient, uint256 amount) external returns (bool)
    ]"#,
);

//tried many ways to use a trait object of the "Signer" trait instead of an enum but it did not work out

#[derive(Error, Debug, Clone)]
pub enum LedgerDeviceError {
    /// HID API error
    #[error("HID API error `{0}`")]
    HidApi(String),
    /// Native HID transport error
    #[error("Native HID transport error `{0}`")]
    NativeTransport(String),
}

#[derive(Clone)]
enum CustomSigner {
    LocalWallet(Wallet<SigningKey>),
    Ledger(H160),
}

static TRANSPORT: Lazy<Arc<Mutex<Option<TransportNativeHID>>>> = Lazy::new(|| Arc::new(Mutex::new(None)));

impl CustomSigner {
    async fn transfer_coin(self,window: &tauri::Window, p: &Provider<Http>, send_to: H160, qnt: u64) -> Result<Option<TransactionReceipt>, String> {
        match self {
            CustomSigner::LocalWallet(lw) =>{
                let client = SignerMiddleware::new(p,lw);
                let tx = TransactionRequest::new().to(send_to).value(qnt);
                //.gas()
                let pending_tx = client.send_transaction(tx, None).await.map_err(|e|{emit_error(&window, &e.to_string()); e.to_string()})?;
                let receipt = pending_tx.await.map_err(|e|{emit_error(&window, &e.to_string()); e.to_string()})?;
                Ok(receipt)
            }
            CustomSigner::Ledger(address) => {
              
                /*
                let ledger = Ledger::new(HDPath::LedgerLive(0), 1).await.map_err(|e|{emit_secondary_error(&window, &e.to_string()); e.to_string()})?;
                if ledger.address() != address{
                    return Err("wrong wallet connected.".to_string());
                }
                let provider = Provider::new(Ws::connect("ws://localhost:8545").await.map_err(|e|{emit_secondary_error(&window, &e.to_string()); e.to_string()})?);
                let client = SignerMiddleware::new(provider, ledger);
                let tx = TransactionRequest::new().to(send_to).value(qnt);
                let pending_tx = client.send_transaction(tx, None).await.map_err(|e|{emit_secondary_error(&window, &e.to_string()); e.to_string()})?;
                let receipt = pending_tx.await.map_err(|e|{emit_secondary_error(&window, &e.to_string()); e.to_string()})?;
                Ok(receipt)
                */
                Ok(None)
            }
        }
    }
    async fn transfer_token(self,window: &tauri::Window, p: &Provider<Http>, send_to: H160, contract_address: H160, qnt: u64) ->  Result<Option<TransactionReceipt>, String> {
        match self {
            CustomSigner::LocalWallet(lw) =>{
                let client = Arc::new(SignerMiddleware::new(p,lw));
                let contract_interface = IERC20::new(contract_address, client);
                let tx = contract_interface.transfer(send_to, qnt.into());
                let pending_tx = tx.send().await.map_err(|e|{emit_error(&window, &e.to_string()); e.to_string()})?;
                let receipt = pending_tx.await.map_err(|e|{emit_error(&window, &e.to_string()); e.to_string()})?;
                Ok(receipt)
            }
            CustomSigner::Ledger(address) => {
                /* 
                let ledger = Ledger::new(HDPath::LedgerLive(0), 1).await.map_err(|e|{emit_secondary_error(&window, &e.to_string()); e.to_string()})?;
                if ledger.address() != address{
                    return Err("wrong wallet connected.".to_string());
                }
                let provider = Provider::new(Ws::connect("ws://localhost:8545").await.map_err(|e|{emit_secondary_error(&window, &e.to_string()); e.to_string()})?);
                let client = Arc::new(SignerMiddleware::new(provider, ledger));
                let contract_interface = IERC20::new(contract_address, client);
                let tx = contract_interface.transfer(send_to, qnt.into());
                let pending_tx = tx.send().await.map_err(|e|{emit_secondary_error(&window, &e.to_string()); e.to_string()})?;
                let receipt = pending_tx.await.map_err(|e|{emit_secondary_error(&window, &e.to_string()); e.to_string()})?;
                Ok(receipt)
                */
                Ok(None)
            }
        }
    }
    
}


#[derive(Default)]
struct UserWallet(Option<CustomSigner>);

#[derive(Default)]
struct NetworkRunning{
    pub network_details:Option<NetworkDetails>,
    pub provider:Option<Provider<Http>>,
    pub tx_token: Option<Sender<Token>>,
    pub tx_quit_controller: Option<Sender<()>>,
    pub tx_quit_coin_balance: Option<Sender<()>>,
    pub contracts_running:Option<HashMap<String,Details>>,
                        //contract-(channel to quit and Token details)
}

struct Details{
    tx_quit: Sender<()>,
    token: Arc<RwLock<Token>>
}

#[derive(Serialize, Deserialize, Clone)]
struct Token{
    name: String,
    contract: String,
    symbol: String,
    decimals: u32,
    quantity: f64
}

#[derive(Serialize, Deserialize,Clone,Debug)]
struct NetworkDetails{
    id: u64,
    currency_symbol: String,
    name: String,
    provider_url: String,
    decimals: u32,
    quantity: f64,
    block_explorer_url: String
}

#[derive(Serialize, Deserialize)]
struct NetworkPriceResp{
    //price:  f32 TODO
}

#[derive(Serialize, Deserialize,Clone)]
struct Network{
    details: NetworkDetails,
    tokens: HashMap<String,Token>
}

#[derive(Serialize, Deserialize)]
struct AppConfig {
    networks: HashMap<u64,Network>
}

//all tauri commands needs to return  Result<(),String> type
//if i return an error i can't for some reason get the string with the error in the frontend
//was necessary to use the window event system to send the errors to frontend

#[tauri::command]
async fn wallet(window: tauri::Window, path: &str, password: &str, wtype: &str, wallet: tauri::State<'_, Arc<RwLock<UserWallet>>>, _network: tauri::State<'_, Arc<RwLock<NetworkRunning>>>) -> Result<(),String> {
    // Open the file
    let public_address: String;
    let mut w = wallet.write().await;
    let mut rng = rand::thread_rng();
    
    // match wtype.as_str()
    match wtype {
    "keystore" =>{
        match File::open(path){
            Err(y)=>{
                error!("{y}");
                let lw = LocalWallet::new(& mut rng);
                public_address = u8_to_string(lw.address().as_bytes());
                rng.fill_bytes(lw.signer().to_bytes().as_mut()); 
                w.0 = Some(CustomSigner::LocalWallet(LocalWallet::encrypt_keystore(path, & mut rng, lw.signer().to_bytes() , password,
                None).map_err(|e|{emit_error(&window, &e.to_string()); e.to_string()})?.0));

            }
            Ok(_) => {
                let lw = LocalWallet::decrypt_keystore(path, password)
                    .map_err(|e|{emit_error(&window, &e.to_string()); e.to_string()})?;
                public_address = u8_to_string(lw.address().as_bytes());
                w.0 =  Some(CustomSigner::LocalWallet(lw));
            }
        }
    }
    "mnemonic" =>{
        match File::open(path){
            Err(y)=>{
                //TODO check error
                //properly check other errors too
    
                // Generate a random wallet (24 word phrase) at custom derivation path
                let lw = MnemonicBuilder::<English>::default()
                    .word_count(24)
                    .derivation_path("m/44'/60'/0'/2/1").map_err(|e|{emit_error(&window, &e.to_string()); e.to_string()})?
                        // Optionally add this if you want the generated mnemonic to be written
                    .password(password)
                    .write_to(path)
                    .build_random(&mut rng).map_err(|e|{emit_error(&window, &e.to_string()); e.to_string()})?;

                public_address = format!("{}",lw.address().clone());
                eprintln!("Random wallet: {:?}",lw.clone());
                w.0 = Some(CustomSigner::LocalWallet(lw));
            },
            Ok(file)=>{
                let reader = io::BufReader::new(file);
                // Read lines from the file
                let mut lines = reader.lines();
                match lines.next() {
                    Some(line) =>{
                        let phrase = line.map_err(|e|{emit_error(&window, &e.to_string()); e.to_string()})?;
                        let index = 0u32;
                        // Access mnemonic phrase with password
                        // Child key at derivation path: m/44'/60'/0'/0/{index}
                        let lw = MnemonicBuilder::<English>::default()
                        .phrase(phrase.as_str())
                        .index(index).map_err(|e|{emit_error(&window, &e.to_string()); e.to_string()})?
                        // Use this if your mnemonic is encrypted
                        .password(password)
                        .build().map_err(|e|{emit_error(&window, &e.to_string()); e.to_string()})?;

                        public_address = format!("{}",lw.address().clone());
                        eprintln!("Wallet: {:?}",lw.clone());
                        w.0 = Some(CustomSigner::LocalWallet(lw));
                    },
                    None => {
                        println!("File does not contain mnemonic phrase.");
                        return Err("no mnemonic phrase found.".to_string());
                    }
                };
            }
        };
    }
    
    "ledger" =>{
        return Err("not supported yet.".to_string());
        /* 
        let ledger = TransportNativeHID::new(hidapi().unwrap()).expect("Could not get a device");

        let command = APDUCommand {
            cla: 0x80,
            ins: 0x02,
            p1: 0x00,
            p2: 0x00,
            data: challenge.as_bytes().clone(),
        };

        let result = ledger.exchange(&command).unwrap();
        */
        /*
        let binding = TRANSPORT.lock().expect("lock exists");
        let transport = binding.as_ref().expect("transport exists");

        let challenge = SecretKey::from_slice(&[0xcd; 32]).unwrap();
        let command = APDUCommand {
            cla: 0x80,
            ins: Instruction::Sign.as_u8(),
            p1: 0x00,
            p2: 0x00,
            data: challenge.secret_bytes().clone(),
        };
        let result = match transport.exchange(&command) {
            Ok(result) => result,
            Err(e) => {
                println!("\nError: Sign {}\n", e);
                Ok(());
            },
        };
        if result.data().len() < 97 {
            println!("\nError: 'Sign' insufficient response! ({:?})\n", result);
            Ok(());
        }
    
        let public_key = &result.data()[1..33];
        let public_key = RistrettoPublicKey::from_bytes(public_key).unwrap();
    
        let sig = &result.data()[33..65];
        let sig = RistrettoSecretKey::from_bytes(sig).unwrap();
    
        let nonce = &result.data()[65..97];
        let nonce = RistrettoPublicKey::from_bytes(nonce).unwrap();
    
        let s = LocalWallet::new(rand::thread_rng());
        
        */
    }
    &_ =>{return Err("no wallet type provided.".to_owned())}

    }
    window
        .emit_all(
            "public-address",
            &public_address
        ).unwrap();

    return Ok(());
}

fn hidapi() -> Result<&'static HidApi, LedgerDeviceError> {
    static HIDAPI: Lazy<Result<HidApi, String>> =
        Lazy::new(|| HidApi::new().map_err(|e| format!("Unable to get HIDAPI: {}", e)));

    HIDAPI.as_ref().map_err(|e| LedgerDeviceError::HidApi(e.to_string()))
}

// start - network change
#[tauri::command]
async fn network(window: tauri::Window, id: u64, address: &str, wallet: tauri::State<'_, Arc<RwLock<UserWallet>>>, network: tauri::State<'_, Arc<RwLock<NetworkRunning>>>) -> Result<(),String>{
    let public_address = address.parse::<Address>().unwrap();
    let config = load_config(&window).map_err(|e|{emit_critical_error(&window, &e.to_string()); e.to_string()})?;

    // check config file for provider if not found use 
    let provider = Provider::<Http>::connect(config.networks[&id].details.provider_url.as_str()).await;
    let (tx_balance, rx_balance) = mpsc::channel(1);
    let (tx_token, rx_token) = mpsc::channel::<Token>(1);
    let (tx_quit_controller, rx_quit_controller) = mpsc::channel(1); 
    
    {
    let mut w = wallet.write().await;
    if let CustomSigner::LocalWallet(lw) = w.0.as_ref().unwrap(){
        w.0 = Some(CustomSigner::LocalWallet(LocalWallet::new_with_signer(lw.signer().clone(), lw.address(), id)));
    } 
    }   
    {
        let mut n = network.write().await;

        //exit the green threads if they are running
        if let Some(tx) = &n.tx_quit_coin_balance{
            let _ = tx.send(()).await;
            let _ = n.tx_quit_controller.as_ref().unwrap().send(()).await;
        }
        n.network_details = Some(NetworkDetails { 
            id: id,
            currency_symbol: config.networks[&id].details.currency_symbol.clone(),
            name: config.networks[&id].details.name.clone(), 
            provider_url: config.networks[&id].details.provider_url.clone(), 
            decimals: config.networks[&id].details.decimals, 
            quantity: config.networks[&id].details.quantity,
            block_explorer_url: config.networks[&id].details.block_explorer_url.clone(),
        });
        n.provider = Some(provider.clone());
        n.tx_quit_coin_balance = Some(tx_balance);
        n.tx_quit_controller= Some(tx_quit_controller);
        n.tx_token= Some(tx_token.clone());
        n.contracts_running = Some(HashMap::new());
    }

    //spawn the tokens controller green thread 
    tokio::task::spawn(controller(window.clone(), provider.clone(), public_address, network.inner().clone(), rx_quit_controller, rx_token));

    if !config.networks[&id].tokens.is_empty(){ 
            for (_, v) in config.networks[&id].tokens.iter(){
                let _ = tx_token.send(v.clone()).await;
            }
        }

   //spawn green thread to fetch the coin balance 
   tokio::task::spawn(network_balance_fetching(window.clone(), provider, public_address, rx_balance, network.inner().clone()));

   return Ok(());
}

async fn controller(window: tauri::Window, provider: Provider<Http>, address: H160, network: Arc<RwLock<NetworkRunning>>, mut rx: Receiver<()>,mut rx_token: Receiver<Token>){

    loop{
        tokio::select! {
            t=rx_token.recv()=>{
                // spawn green threads for every contract to watch and fetch balance,
                // save every channel so we can quit from those functions when necessary
                // track and save the state of the tokens 
                let mut n = network.write().await;
                let (tx, rx) = mpsc::channel::<()>(1);
                let token = Arc::new(RwLock::new(t.as_ref().unwrap().clone()));
                n.contracts_running.as_mut().unwrap().insert(t.unwrap().contract.clone(),Details{token: token.clone(),tx_quit:tx});

                tokio::task::spawn(contract_watch(window.clone(), provider.clone(), address, token.clone(), rx));
            }
            _ = rx.recv() => {
                let n = network.write().await;
                if n.contracts_running.is_some(){ // check if running is empty
                    for (_, v) in n.contracts_running.as_ref().unwrap(){
                        let _ = v.tx_quit.send(()).await;
                    }
                }
                
                return 
            }
        }
    }
}

// watch the contract for transactions and fetch the balance
async fn contract_watch(window: tauri::Window, provider: Provider<Http>, address: H160, token: Arc<RwLock<Token>>, mut rx: Receiver<()>) {
    // watch for transfer events and only check for balance after that
    emit_token_balance(window.clone(),provider.clone(),address,token.clone()).await;

    let filter1 = Filter::new()
        .address(token.read().await.contract.clone().parse::<Address>().unwrap())
        .event("event Transfer(address indexed from, address indexed to, uint256 value)")
        .topic2(vec![address]);
    
    let filter2 = Filter::new()
        .address(token.read().await.contract.clone().parse::<Address>().unwrap())
        .event("event Transfer(address indexed from, address indexed to, uint256 value)")
        .topic1(vec![address]);
    

    loop{
        tokio::select! {
            mut events = provider.watch(&filter1) =>{
                while let Some(_event) = events.as_mut().unwrap().next().await{
                    emit_token_balance(window.clone(),provider.clone(),address,token.clone()).await;
                } 
            }

            mut events = provider.watch(&filter2) =>{
                while let Some(_event) = events.as_mut().unwrap().next().await{
                    emit_token_balance(window.clone(),provider.clone(),address,token.clone()).await;
                } 
            }
            
            _ = rx.recv() => {
                return ;
            }
        }
    }
}

// function for fetching the balance of coins in the network from the wallet
async fn network_balance_fetching(window: tauri::Window, provider: Provider<Http>, address: H160, mut rx: Receiver<()> ,network: Arc<RwLock<NetworkRunning>>) {
    let mut ticker = interval(Duration::from_secs(60));

    emit_network_balance(window.clone(), provider.clone(), address, network.clone()).await;

    loop {
        tokio::select! {
            _ = ticker.tick() =>{
                emit_network_balance(window.clone(), provider.clone(), address, network.clone()).await;
            }
            _ = rx.recv() => {
                return ;
            }
        }
    }
}
#[tokio::main]
async fn main() {
    let stdout = ConsoleAppender::builder().build();    let config = Config::builder()
        .appender(Appender::builder().build("stdout", Box::new(stdout)))
        .build(Root::builder().appender("stdout").build(LevelFilter::Trace))
        .unwrap();    
    
    let _handle = log4rs::init_config(config).unwrap();
    
    let wallet_state = Arc::new(RwLock::new(UserWallet::default()));
    let network_state = Arc::new(RwLock::new(NetworkRunning::default()));
    
    tauri::Builder::default()
        .manage(wallet_state).manage(network_state)
        .invoke_handler(tauri::generate_handler![wallet,network,cointransfer,tokentransfer,newcontract,new_network])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

async fn emit_network_balance(window: tauri::Window, provider: Provider<Http>, address: H160, network: Arc<RwLock<NetworkRunning>>) {
    match provider.get_block(BlockNumber::Latest).await.unwrap_or(None){
        None => (),
        Some(b) => {
            match provider.get_balance(address, Some(BlockId::from(b.number.unwrap()))).await{
                Ok(qnt)=>{
                    println!("{:?}",qnt.clone());
                    let decimals = &network.read().await.network_details.as_ref().unwrap().decimals.clone();
                    network.write().await.network_details.as_mut().unwrap().quantity = float_decimals(qnt.as_u64(),
                     decimals);
                     println!("{:?}",&network.read().await.network_details.as_ref().unwrap().clone());
                    window 
                        .emit_all(
                            "network-details",
                            &network.read().await.network_details.as_ref().unwrap().clone()
                        ).unwrap();
                },
                Err(y)=> error!("{y}"),
            }
        }
    };
}

async fn emit_token_balance(window: tauri::Window, provider: Provider<Http>, address: H160, token: Arc<RwLock<Token>>) {
    let contract_address: Address = token.read().await.contract.clone().parse().unwrap();
    let contract = IERC20::new(contract_address,provider.clone().into());

    match contract.balance_of(address).await{
        Ok(balance) =>{
            let decimals = token.read().await.decimals.clone();
            token.write().await.quantity = float_decimals(balance.as_u64(), &decimals);
            window 
                .emit_all(
                    "token-details",
                    &token.read().await.clone()
                ).unwrap();
            },
        Err(y)=>error!("{y}"),
    }

}

#[tauri::command]
async fn cointransfer(window: tauri::Window, myaddress: &str, to: &str, qnt: &str, wallet: tauri::State<'_, Arc<RwLock<UserWallet>>>, network: tauri::State<'_, Arc<RwLock<NetworkRunning>>>) -> Result<(),String> {
    let provider = network.read().await.provider.as_ref().unwrap().clone();
    let receipt =  wallet.read().await.0.as_ref().unwrap().clone().transfer_coin(&window, &provider, to.parse().unwrap(), string_to_u64_with_decimals(qnt,network.read().await.network_details.as_ref().unwrap().decimals.clone())).await?;
    emit_network_balance(window.clone(), provider , myaddress.parse().unwrap(), network.inner().clone()).await;
    parse_receipt(window,
         receipt
    ).await
}


#[tauri::command]
async fn tokentransfer(window: tauri::Window, contract: &str, to: &str, qnt: &str, decimals: u32, wallet: tauri::State<'_, Arc<RwLock<UserWallet>>>, network: tauri::State<'_, Arc<RwLock<NetworkRunning>>>) -> Result<(),String>{
    parse_receipt(window.clone(),
        wallet.read().await.0.as_ref().unwrap().clone().transfer_token(
            &window, &network.read().await.provider.clone().unwrap(), to.parse().unwrap(), contract.parse().unwrap(), string_to_u64_with_decimals(qnt, decimals)
        ).await?).await
}

async fn parse_receipt(window: tauri::Window, receipt: Option<TransactionReceipt>)-> Result<(),String>{
        match receipt{
        Some(tx)=>{
            let msg = format!("0x{}", u8_to_string(tx.transaction_hash.as_bytes()));
            debug!("{}",msg.clone());
            window 
            .emit_all(
                "tx-hash",
                &msg
            ).unwrap();
            
            return Ok(())   
        }
        None=>{
            error!("tx dropped from mempool"); 
            return Err("tx dropped from mempool".to_owned());
        }
    };
}

#[tauri::command]
async fn new_network(window: tauri::Window, chain_id: &str, name: &str, currency_symbol: &str, provider_url: &str, decimals: &str, block_explorer_url: &str, _wallet: tauri::State<'_, Arc<RwLock<UserWallet>>>, _network: tauri::State<'_, Arc<RwLock<NetworkRunning>>>) -> Result<(),String> {
    let mut config = load_config(&window).map_err(|e|{emit_error(&window, &e.to_string()); e.to_string()})?;

    config.networks.insert(chain_id.parse().unwrap(), Network { 
        details: NetworkDetails{
            id: chain_id.parse().unwrap(),
            currency_symbol: currency_symbol.to_string(),
            name: name.to_string(),
            provider_url: provider_url.to_string(),
            decimals: decimals.parse().unwrap(),
            quantity:0.0,
            block_explorer_url:block_explorer_url.to_string()
        }, 
        tokens: HashMap::new() 
    });

    save_config(&config).map_err(|e|{emit_error(&window, &e.to_string()); e.to_string()})?;

    Ok(())
}

#[tauri::command]
async fn newcontract(window: tauri::Window, id: u64, name: &str, contract: &str, symbol: &str, decimals:&str, _wallet: tauri::State<'_, Arc<RwLock<UserWallet>>>, network: tauri::State<'_, Arc<RwLock<NetworkRunning>>>) -> Result<(),String>{
    let mut config = load_config(&window).map_err(|e|{emit_error(&window, &e.to_string()); e.to_string()})?;

    config.networks.get_mut(&id).unwrap().tokens.insert(contract.to_owned(),
    Token { name: name.to_owned(), 
        contract: contract.to_owned(), 
        symbol: symbol.to_owned(), 
        decimals: decimals.parse().unwrap(), 
        quantity: 0.0}
    );
    
    save_config(&config).map_err(|e|{emit_error(&window, &e.to_string()); e.to_string()})?;

    let channel: Sender<Token>;
    {
    let n = network.read().await;
    channel = n.tx_token.as_ref().unwrap().clone();
    }

    let _ = channel.send(
        Token{name:name.to_owned(),
            contract:contract.to_owned(),
            symbol:symbol.to_owned(),
            decimals:decimals.parse().unwrap(),
            quantity:0.0}
    ).await;

    Ok(())
}



fn emit_critical_error(window: &tauri::Window, err: &str){
    error!("{err}");
    window 
        .emit_all(
            "critical-error",
            err
        ).unwrap();
}
    
fn emit_error(window: &tauri::Window, err: &str){
    error!("{err}");
    window 
        .emit_all(
            "error",
            err
        ).unwrap();
}

// Function to save the configuration to a file
fn save_config(config: &AppConfig) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(config_path) = get_config_path() {
        let config_str = serde_json::to_string(config)?;
        fs::write(config_path, config_str)?;
    }
    Ok(())
}

// Function to load the configuration from a file
fn load_config(window: &tauri::Window) -> Result<AppConfig, Box<dyn std::error::Error>> {
    let config_path = get_config_path(); 
    if let Some(config_path) = config_path.clone() {
        if let Ok(config_str) = fs::read_to_string(config_path) {
            let config: AppConfig = serde_json::from_str(&config_str)?;
            return Ok(config);
        }
    }

    // Fallback to default configuration if file doesn't exist or can't be read 
    let config = AppConfig {
        networks: HashMap::from([(5,
            Network{
                details:NetworkDetails { 
                    id:5,
                    name:"Ethereum".to_string(),
                    currency_symbol: "Ether".to_string(),
                    provider_url:"https://goerli.infura.io/v3/0ea278dac686455e85637348edb5d566".to_string(),
                    decimals: 18,
                    quantity:0.0, 
                    block_explorer_url:"https://etherscan.io".to_string()
                },
                tokens:HashMap::new()
                })])
    };

    if let Err(e) = create_dir_all(&config_path.clone().unwrap().parent().unwrap()) {
        return Err(Box::new(e));
    }

    let mut file = match File::create(config_path.clone().clone().unwrap()) {
        Ok(file) => file,
        Err(e) => {
            return Err(Box::new(e));
        }
    };

    let empty_json = json!({});
    let empty_json_string = serde_json::to_string_pretty(&empty_json).unwrap();
    if let Err(e) = file.write_all(empty_json_string.as_bytes()) {
        return Err(Box::new(e));
    }

    save_config(&config).map_err(|e|{emit_error(&window, &e.to_string()); e.to_string()})?;

    Ok(config)
}
// Function to get the configuration file path
fn get_config_path() -> Option<PathBuf> {
    match env::var_os("APPDATA") {
        Some(app_data) => Some(PathBuf::from(app_data).join("evm-wallet/config.json")),
        None => {
            // Fallback to home directory for Linux or user's profile directory for Windows
            let mut config_dir = dirs::home_dir()?;
            config_dir.push("evm-wallet");
            Some(config_dir.join("config.json"))
        }
    }
}

fn float_decimals(value: u64, decimals: &u32) -> f64 {
    // Convert the u256 value to a f64
    let float_value = value as f64;

    // Adjust for the number of decimals
    let divisor = 10u64.pow(decimals.to_owned());
    let result = float_value / divisor as f64;

    result
}

fn string_to_u64_with_decimals(float_str: &str, decimals: u32) -> u64 {
    let float: f64 = float_str.parse().unwrap();
    let multiplier = 10u64.pow(decimals);
    (float * multiplier as f64) as u64

}

fn u8_to_string(bytes: &[u8]) -> String{
    let address_str = bytes
        .iter()
        .map(|byte| format!("{:02X}", byte))
        .collect::<Vec<String>>()
        .concat();
    
    format!("0x{address_str}")
}
