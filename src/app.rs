use leptos::leptos_dom::ev::MouseEvent;
use leptos::*;
use serde::{Deserialize, Serialize};
use serde_wasm_bindgen::to_value;
use tauri_sys::{event, tauri};
use wasm_bindgen::prelude::*;
use web_sys::SubmitEvent;
use core::f64;
use std::collections::HashMap;
use futures::stream::StreamExt;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "tauri"])]
    async fn invoke(cmd: &str, args: JsValue) -> JsValue;
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Token{
    name: String,
    contract: String,
    symbol: String,
    decimals: u32,
    quantity: f64
}

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
struct NetworkDetails{
    id: u64,
    currency_symbol: String,
    name: String,
    provider_url: String,
    decimals: u32,
    quantity: f64,
    block_explorer_url: String,
}

#[derive(Serialize, Deserialize)]
struct NetworkArgs<'a> {
    id: u64,
    address: &'a str,
}

#[derive(Serialize, Deserialize)]
struct WalletArgs<'a> {
    path: &'a str,
    password: &'a str,
    wtype: &'a str,
}

#[derive(Serialize, Deserialize, Default,Clone)]
struct NewNetworkArgs {
    chain_id: String,
    name: String,
    currency_symbol: String,
    provider_url: String,
    decimals: String,
    block_explorer_url: String,
}

#[derive(Serialize, Deserialize, Default,Clone)]
struct NewTokenArgs {
    id: u64,
    name: String,
    contract: String,
    symbol: String,
    decimals: String
}

#[derive(Serialize, Deserialize)]
struct SendCoinArgs<'a> {
    myaddress: &'a str,
    to: &'a str,
    qnt: &'a str,
}


#[derive(Serialize, Deserialize)]
struct SendTokenArgs<'a> {
    myaddress: &'a str,
    contract: &'a str,
    to: &'a str,
    qnt: &'a str,
    decimals: u32,
}

async fn listen_on_contracts_update(event_writer: WriteSignal<HashMap<String,Token>>) {
    let mut events = event::listen::<Token>("token-details")
        .await
        .unwrap();

    while let Some(event) = events.next().await {
        log::debug!("Received contract-event {:#?}", event);
        event_writer.update(|all_contracts| {all_contracts.insert(event.payload.contract.clone(), event.payload);()});
    }
}

async fn listen_on_network_update(event_writer: WriteSignal<NetworkDetails>) {
    let mut events = event::listen::<NetworkDetails>("network-details")
        .await
        .unwrap();

    while let Some(event) = events.next().await {
        log::debug!("Received network-event {:#?}", event);
        event_writer.set(event.payload);
    }
}

async fn listen_on_wallet_update(event_writer: WriteSignal<String>) {
    let mut events = event::listen::<String>("public-address")
        .await
        .unwrap();

    while let Some(event) = events.next().await {
        log::debug!("Received public-address-event {:#?}", event);
        event_writer.set(event.payload);
    }
}

async fn listen_on_errors(event_writer: WriteSignal<String>) {
    let mut events = event::listen::<String>("error")
        .await
        .unwrap();

    while let Some(event) = events.next().await {
        log::debug!("Received error {:#?}", event);
        event_writer.set(event.payload);
    }
}

async fn listen_on_critical_error(event_writer: WriteSignal<String>) {
    let mut events = event::listen::<String>("critical-error")
        .await
        .unwrap();

    while let Some(event) = events.next().await {
        log::debug!("Received critical-error {:#?}", event);
        event_writer.set(event.payload);
    }
}

async fn listen_on_txhash(event_writer: WriteSignal<String>) {
    let mut events = event::listen::<String>("tx-hash")
        .await
        .unwrap();

    while let Some(event) = events.next().await {
        log::debug!("Received tx-hash {:#?}", event);
        event_writer.set(event.payload);
    }
}

#[component]
pub fn App() -> impl IntoView {
    let (path, set_path) = create_signal(String::new());
    let (password, set_password) = create_signal(String::new());
    let (error_msg, set_error_msg) = create_signal(String::from(String::new()));
    let (critical_error, set_critical_error) = create_signal(String::new());
    let (network_id, set_networkd_id) = create_signal(5);
    let (toggle_token, set_toggle_token) = create_signal(String::new());
    let (toggle_token_decimals, set_toggle_token_decimals) = create_signal(0);
    let (toggle_coin, set_toggle_coin) = create_signal(false);
    let (qnt_send, set_qnt_send) = create_signal(String::new());
    let (address_send, set_address_send) = create_signal(String::new());
    let (new_network_toggle, set_new_network_toggle) = create_signal(false);
    let (new_token_toggle, set_new_token_toggle) = create_signal(false);
    let (new_network, set_new_network) = create_signal(NewNetworkArgs::default());
    let (new_token, set_new_token) = create_signal(NewTokenArgs::default());

    let (event_token_map, set_event_token_map) = create_signal::<HashMap<String,Token>>(HashMap::new());
    create_local_resource(move || set_event_token_map , listen_on_contracts_update);

    let (event_network, set_event_network) = create_signal(NetworkDetails::default());
    create_local_resource(move || set_event_network, listen_on_network_update);

    let (wallet_address, set_wallet_address) = create_signal(String::new());
    create_local_resource(move || set_wallet_address, listen_on_wallet_update);

    create_local_resource(move || set_critical_error, listen_on_critical_error);
    create_local_resource(move || set_error_msg, listen_on_errors);

    let update_send_address = move |ev| {
        let v = event_target_value(&ev);
        set_address_send.set(v);
    };

    let update_qnt_send = move |ev| {
        let v = event_target_value(&ev);
        set_qnt_send.set(v);
    };

    let update_password = move |ev| {
        let v = event_target_value(&ev);
        set_password.set(v);
    };

    let update_path = move |ev| {
        let v = event_target_value(&ev);
        set_path.set(v);
    };

    let enough_balance= move |qnt_send: f64,balance: f64| -> bool {
        if balance > qnt_send {
            return true
        }
        false
    };

    let save_new_token = move |ev: SubmitEvent| {
        ev.prevent_default();
       if new_token.get().name.is_empty() | new_token.get().symbol.is_empty() | new_token.get().contract.is_empty() | new_token.get().decimals.is_empty() {
            return
       }
       spawn_local(async move {
            set_new_token.update(|t|t.id = network_id.get());
            let args = to_value(&new_token.get()).unwrap();
            let _signal = invoke("newcontract", args).await.as_string();
            set_new_token.set(NewTokenArgs::default());

        });
       
    };

    let save_new_network = move |ev: SubmitEvent| {
        ev.prevent_default();
       if new_network.get().chain_id.is_empty() | new_network.get().name.is_empty() | new_network.get().currency_symbol.is_empty() | new_network.get().decimals.is_empty() | new_network.get().provider_url.is_empty() | new_network.get().block_explorer_url.is_empty() {
            return
       }
       spawn_local(async move {
            let args = to_value(&new_network.get()).unwrap();
            let _signal = invoke("new_network", args).await.as_string();
            set_new_network.set(NewNetworkArgs::default());
        });
    };

    let token_transaction = move |ev: SubmitEvent| {
        ev.prevent_default();
        if address_send.get().is_empty() | qnt_send.get().is_empty() {
            return;
        }
        if !enough_balance(qnt_send.get().parse().unwrap(),event_token_map.get()[&toggle_token.get()].quantity){
            set_error_msg.set("not enough balance".to_owned());
            return;
        }
        set_error_msg.set(String::new());set_toggle_token.set(String::new());
        spawn_local(async move {
            let args = to_value(&SendTokenArgs{myaddress: &wallet_address.get(),contract: &toggle_token.get(),to: &address_send.get(), qnt: &qnt_send.get(), decimals: toggle_token_decimals.get()} ).unwrap();
            let _signal = invoke("tokentransfer", args).await.as_string();
        });
    };

    let coin_transaction = move |ev: SubmitEvent| {
        ev.prevent_default();
        if address_send.get().is_empty() | qnt_send.get().is_empty() {
            return;
        }
        if !enough_balance(qnt_send.get().parse().unwrap(),event_network.get().quantity){
            set_error_msg.set("not enough balance".to_owned());
            return;
        }
        set_error_msg.set(String::new());set_toggle_coin.set(false);
        spawn_local(async move {
            let args = to_value(&SendCoinArgs{myaddress: &wallet_address.get(), to: &address_send.get(), qnt: &qnt_send.get()}).unwrap();
            let _signal = invoke("cointransfer", args).await.as_string();
        });
    };

    let network = move |id:u64| {
        spawn_local(async move {
            let args = to_value(&NetworkArgs{id, address:&wallet_address.get()}).unwrap();
            let _signal = invoke("network", args).await.as_string(); 
        });
    };

    let wallet = move |ev: MouseEvent, t : String| {
        ev.prevent_default();
        log::debug!("{}",path.get().as_str());
        log::debug!("{}",password.get().as_str());
        spawn_local(async move {
            if path.get().is_empty() | password.get().is_empty() {
                return;
            }
            let args = to_value(&WalletArgs{path: &path.get(), password: &password.get(), wtype: &t}).unwrap();
            let _signal = invoke("wallet", args).await.as_string();
        });
    };

    

    create_effect(move |_| {
        if !wallet_address.get().is_empty(){
            set_error_msg.set(String::new());
            log::debug!("address: {}", wallet_address.get());
            network(network_id.get());
            log::debug!("network id: {}",network_id.get());
        }
    });

    view! { 
        <main class="container">
        {move || if !critical_error.get().is_empty(){
                view! {
                    <div>
                    <h1>"Critical Error:"</h1><br/>
                    <h2>{critical_error.get()}</h2>
                    </div>
                }
            }else if new_network_toggle.get(){
                view! {
                    <div>
                        <form class="column" on:submit=save_new_network>
                            <input
                                id="chain-id-input"
                                placeholder="Chain ID"
                                on:input=move |ev| { ev.prevent_default(); set_new_network.update(|n|n.chain_id = event_target_value(&ev))}
                            />
                            <input
                                id="name-input"
                                placeholder="Name"
                                on:input=move |ev| { ev.prevent_default(); set_new_network.update(|n|n.name = event_target_value(&ev))}
                            />
                            <input
                                id="symbol-input"
                                placeholder="Symbol"
                                on:input=move |ev| { ev.prevent_default(); set_new_network.update(|n|n.currency_symbol = event_target_value(&ev))}
                            />
                            <input
                                id="decimals-input"
                                placeholder="Decimals"
                                on:input=move |ev| { ev.prevent_default(); set_new_network.update(|n|n.decimals = event_target_value(&ev))}
                            />
                            <input
                                id="provider-input"
                                placeholder="Provider URL"
                                on:input=move |ev| { ev.prevent_default(); set_new_network.update(|n|n.provider_url = event_target_value(&ev))}
                            />
                            <input
                                id="explorer-input"
                                placeholder="Explorer URL"
                                on:input=move |ev| { ev.prevent_default(); set_new_network.update(|n|n.block_explorer_url = event_target_value(&ev))}
                            />
                            <button type="submit">"ADD NEW NETWORK"</button>
                        </form>
                    <br/> <br/>
                    <button class="bottom" on:click= move |_| {set_new_network.set(NewNetworkArgs::default());set_new_network_toggle.set(false)}>"go back"</button>
                    </div>
                }
            }else if new_token_toggle.get(){
                view! {
                    <div class="middle">
                    <form class="column" on:submit=save_new_token>
                        <input
                            id="address-input"
                            placeholder="Contract Address"
                            on:input=move |ev| { ev.prevent_default(); set_new_token.update(|t|t.contract = event_target_value(&ev))}
                        />
                        <input
                            id="name-input"
                            placeholder="Name"
                            on:input=move |ev| { ev.prevent_default(); set_new_token.update(|t|t.name = event_target_value(&ev))}
                        />
                        <input
                            id="symbol-input"
                            placeholder="Symbol"
                            on:input=move |ev| { ev.prevent_default(); set_new_token.update(|t|t.symbol = event_target_value(&ev))}
                        />
                        <input
                            id="decimals-input"
                            placeholder="Decimals"
                            on:input=move |ev| { ev.prevent_default(); set_new_token.update(|t|t.decimals = event_target_value(&ev))}
                        />
                        <button type="submit">"ADD NEW TOKEN"</button>
                    </form>
                    <br/> <br/>
                    <button class="bottom" on:click=move |_| {set_new_token.set(NewTokenArgs::default());set_new_token_toggle.set(false)}>"go back"</button>
                </div>
                }
            }else if toggle_coin.get(){
                view! {
                    <div class="middle">
                        <div>"balance: "<h2>{move || event_network.get().quantity}" "{move || event_network.get().currency_symbol.to_uppercase()}</h2></div>
                        <br/>
                        <form class="column" on:submit=coin_transaction>
                            <input
                                id="address-input"
                                placeholder="Address to send"
                                on:input=update_send_address
                            />
                            <input
                                id="quantity-input"
                                placeholder="amount"
                                on:input=update_qnt_send
                            />
                            <button type="submit">"SEND"</button>
                        </form>
                        <br/>
                        <p class="error"><h3>{ move || error_msg.get()}</h3></p>
                        <button class="bottom" on:click=move |_| {set_error_msg.set(String::new());set_toggle_coin.set(false)}>"go back"</button>
                    </div>
                }
            }else if !toggle_token.get().is_empty(){
                view! {
                    <div>
                        <form class="column" on:submit=token_transaction>
                            <input
                                id="address-input"
                                placeholder="Address to send"
                                on:input=update_send_address
                            />
                            <input
                                id="quantity-input"
                                placeholder="amount"
                                on:input=update_qnt_send
                            />
                            <button type="submit">"SEND"</button>
                        </form>
                        <br/> 
                        <p class="error"><h3>{ move || error_msg.get()}</h3></p>
                        <button class="bottom" on:click=move |_| {set_error_msg.set(String::new());set_toggle_token.set(String::new())}>"go back"</button>
                    </div>
                }
            }else if event_network.get().id != 0{
                view! {
                    <div>
                        <div>{ move || wallet_address.get()}</div>
                        <br/>
                        <div>{move || event_network.get().name} " Network"</div>
                        <br/>
                        <div>{move || event_network.get().quantity}" "{move || event_network.get().currency_symbol.to_string()}" "
                        <button on:click=move |_| set_toggle_coin.set(true) >"SEND"</button></div>
                        <br/>
                        
                        <ul>
                        {move || {
                            let token_map = event_token_map.get();
                            let mut elements = Vec::new();
                            
                            for token in token_map.iter() {
                                let name = token.1.name.clone();
                                let quantity = token.1.quantity;
                                let symbol = token.1.symbol.clone();
                                let decimals = token.1.decimals.clone();
                                let contract_address = token.1.contract.clone();
                                
                                elements.push(view! {
                                    <li>
                                        "Token: "{name}
                                        <br/>
                                        {quantity}" "{symbol}" "
                                        <button on:click=move |_| {set_toggle_token.set(contract_address.clone());set_toggle_token_decimals.set(decimals)}>SEND</button>
                                    </li>
                                });
                            }
                            
                            elements
                        }}
                        </ul>
                        <br/>
                        <button on:click=move |_| {
                            set_new_token_toggle.set(true);
                        }>"+"</button>
                        <br/> <br/>
                        <p class="bottom-error"><h3>{ move || error_msg.get()}</h3></p>
                    </div>}
        }else if !wallet_address.get().is_empty(){
            view! {<div>"Connecting"</div> }
        }else{
            view! {
                <div>
                <br/><br/>
            <div class="row">
                <a href="" target="_blank">
                    <img src="https://developers.ledger.com/assets/img/logos/ledger-horizontal.png" class="logo tauri" alt="Ledger Logo"/>
                </a>
                <button on:click={move |ev|{
                    ev.prevent_default();wallet(ev,"ledger".to_owned())}} >"use ledger"</button>
            </div>
            <br/><br/>
            <p>"Use local file:"</p>
            <input
                id="wallet-path"
                placeholder="Enter a path..."
                on:change=update_path
            />
            <input
                id="wallet-password"
                placeholder="Enter a password..."
                on:change=update_password
            />
            <button on:click={move |ev|{
                ev.prevent_default();wallet(ev,"keystore".to_owned())}} >"Keystore"</button>
            <button on:click={move |ev|{
                ev.prevent_default();wallet(ev,"mnemonic".to_owned())}} >"Mnemonic"</button>
            <br/>
            <p class="error"><h3>{ move || error_msg.get()}</h3></p>
            </div> }
        }}
        </main>
    }
}

