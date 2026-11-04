#![allow(dead_code)]
use crate::*;
use near_sdk::mock::MockAction;
use near_sdk::serde_json::{self, Value, json};
use near_sdk::test_utils::{VMContextBuilder, get_created_receipts, get_logs};
use near_sdk::{NearToken, PromiseResult, testing_env};

pub const CONTRACT: &str = "reputation-registry.erc-8004.testnet";
pub const IDENTITY: &str = "identity-registry.erc-8004.testnet";
pub const VERIFIER: &str = "erc8004-verifier.arras-dev.testnet";
pub const CLIENT: &str = "cliente.testnet";
pub const CLIENT2: &str = "cliente2.testnet";
pub const OWNER: &str = "dueno.testnet";
pub const WALLET: &str = "wallet.testnet";
pub const OTHER: &str = "tercero.testnet";
pub const NOW: u64 = 1_793_491_200_000_000_000;
pub const HEIGHT: u64 = 1_000_000;
pub const HASH: &str = "00ff10a0b1c2d3e4f5061728394a5b6c7d8e9fa0b1c2d3e4f5061728394a5b6c";
pub const PID: &str = "DN5eMA3sNES7xsoy6qvy24cVF6Dmwjxpkvd2b4e8P38M";
pub const BYTE: u128 = 10_000_000_000_000_000_000;
pub const NEAR: u128 = 1_000_000_000_000_000_000_000_000;

pub fn acc(s: &str) -> AccountId {
    s.parse().unwrap()
}

/// El storage de un contexto arranca donde quedo el anterior (para medir entre llamadas, con `flush_all` antes).
pub fn ctx_full(predecessor: &str, signer: &str, deposit: u128, height: u64, gas: Gas) {
    let usage = env::storage_usage();
    testing_env!(
        VMContextBuilder::new()
            .storage_usage(usage)
            .current_account_id(acc(CONTRACT))
            .predecessor_account_id(acc(predecessor))
            .signer_account_id(acc(signer))
            .attached_deposit(NearToken::from_yoctonear(deposit))
            .account_balance(NearToken::from_near(10))
            .block_timestamp(NOW)
            .block_height(height)
            .prepaid_gas(gas)
            .build()
    );
}

pub fn ctx(predecessor: &str, deposit: u128) {
    ctx_full(
        predecessor,
        predecessor,
        deposit,
        HEIGHT,
        Gas::from_tgas(300),
    );
}

pub fn ctx_at(predecessor: &str, deposit: u128, height: u64) {
    ctx_full(
        predecessor,
        predecessor,
        deposit,
        height,
        Gas::from_tgas(300),
    );
}

/// Contexto de un callback con estos resultados de promesa.
pub fn ctx_callback(results: Vec<PromiseResult>, height: u64) {
    let usage = env::storage_usage();
    testing_env!(
        VMContextBuilder::new()
            .storage_usage(usage)
            .current_account_id(acc(CONTRACT))
            .predecessor_account_id(acc(CONTRACT))
            .signer_account_id(acc(OTHER))
            .account_balance(NearToken::from_near(10))
            .block_timestamp(NOW + 1_000_000_000)
            .block_height(height)
            .build(),
        near_sdk::test_vm_config(),
        near_sdk::RuntimeFeesConfig::test(),
        Default::default(),
        results
    );
}

pub fn new_contract() -> ReputationRegistry {
    ctx(CONTRACT, 0);
    ReputationRegistry::new(acc(IDENTITY))
}

pub fn ok(v: Value) -> PromiseResult {
    PromiseResult::Successful(serde_json::to_vec(&v).unwrap())
}

pub fn raw(b: &[u8]) -> PromiseResult {
    PromiseResult::Successful(b.to_vec())
}

/// La respuesta de `check_agent` de un agente de `OWNER` con `agentWallet` = `wallet`.
pub fn agent(authorized: bool, wallet: Option<&str>) -> PromiseResult {
    ok(json!({"owner_id": OWNER, "agent_wallet": wallet, "authorized": authorized}))
}

pub fn agent_ok() -> PromiseResult {
    agent(false, Some(OWNER))
}

pub fn proof_resp(counterparty: &str, role: &str, context: &str, weight: u128) -> PromiseResult {
    ok(
        json!({"counterparty": counterparty, "role": role, "context": context, "weight": weight.to_string()}),
    )
}

#[derive(Clone)]
pub struct Pedido {
    pub agent_id: String,
    pub value: String,
    pub value_decimals: u8,
    pub tag1: Option<String>,
    pub tag2: Option<String>,
    pub endpoint: Option<String>,
    pub feedback_uri: Option<String>,
    pub feedback_hash: Option<String>,
}

impl Default for Pedido {
    fn default() -> Self {
        Self {
            agent_id: "7".into(),
            value: "87".into(),
            value_decimals: 0,
            tag1: Some("starred".into()),
            tag2: None,
            endpoint: Some("https://agente.example/mcp".into()),
            feedback_uri: Some("ipfs://bafyfeedback".into()),
            feedback_hash: Some(HASH.into()),
        }
    }
}

pub fn pedido(agent_id: &str, value: &str, decimals: u8, tag1: &str, tag2: &str) -> Pedido {
    Pedido {
        agent_id: agent_id.into(),
        value: value.into(),
        value_decimals: decimals,
        tag1: Some(tag1.into()),
        tag2: Some(tag2.into()),
        endpoint: None,
        feedback_uri: None,
        feedback_hash: None,
    }
}

pub fn call_give(c: &mut ReputationRegistry, p: &Pedido) {
    c.give_feedback(
        p.agent_id.clone(),
        p.value.clone(),
        p.value_decimals,
        p.tag1.clone(),
        p.tag2.clone(),
        p.endpoint.clone(),
        p.feedback_uri.clone(),
        p.feedback_hash.clone(),
    )
    .detach();
}

/// Las llamadas de funcion de las promesas creadas: (receptor, metodo, args JSON, gas estatico, deposito).
pub fn function_calls() -> Vec<(String, String, Value, Gas, u128)> {
    let mut out = vec![];
    for r in get_created_receipts() {
        for a in r.actions {
            if let MockAction::FunctionCallWeight {
                method_name,
                args,
                prepaid_gas,
                attached_deposit,
                ..
            } = a
            {
                out.push((
                    r.receiver_id.to_string(),
                    String::from_utf8(method_name).unwrap(),
                    serde_json::from_slice(&args).unwrap(),
                    prepaid_gas,
                    attached_deposit.as_yoctonear(),
                ));
            }
        }
    }
    out
}

pub fn call_args(method: &str) -> Value {
    function_calls()
        .into_iter()
        .find(|c| c.1 == method)
        .unwrap_or_else(|| panic!("no se creo {method}"))
        .2
}

/// `give_feedback` de `client` con `deposit`; devuelve el `PendingFeedback` que quedo en la promesa.
pub fn give_with(
    c: &mut ReputationRegistry,
    client: &str,
    deposit: u128,
    p: &Pedido,
) -> PendingFeedback {
    ctx(client, deposit);
    call_give(c, p);
    serde_json::from_value(call_args("on_feedback_checked")["p"].clone()).unwrap()
}

pub fn give(c: &mut ReputationRegistry, client: &str, p: &Pedido) -> PendingFeedback {
    give_with(c, client, ReputationRegistry::feedback_deposit(), p)
}

pub fn callback(
    c: &mut ReputationRegistry,
    p: PendingFeedback,
    r: PromiseResult,
) -> Option<String> {
    ctx_callback(vec![r], HEIGHT + 2);
    c.on_feedback_checked(p)
}

/// give + callback con un agente que acepta.
pub fn rate(c: &mut ReputationRegistry, client: &str, p: &Pedido) -> String {
    let args = give(c, client, p);
    callback(c, args, agent_ok()).expect("rechazada")
}

pub fn transfers() -> Vec<(String, u128)> {
    let mut out = vec![];
    for r in get_created_receipts() {
        for a in r.actions {
            if let MockAction::Transfer { deposit, .. } = a {
                out.push((r.receiver_id.to_string(), deposit.as_yoctonear()));
            }
        }
    }
    out
}

pub fn events() -> Vec<(String, Value)> {
    get_logs()
        .iter()
        .filter_map(|l| l.strip_prefix("EVENT_JSON:"))
        .map(|j| {
            let e: Value = serde_json::from_str(j).unwrap();
            assert_eq!(e["standard"], "erc8004");
            assert_eq!(e["version"], "1.0.0");
            (
                e["event"].as_str().unwrap().to_string(),
                e["data"][0].clone(),
            )
        })
        .collect()
}

pub fn logs() -> Vec<String> {
    get_logs()
}
