#![allow(dead_code)]
use crate::*;
use near_sdk::mock::MockAction;
use near_sdk::serde_json::{self, Value};
use near_sdk::test_utils::{VMContextBuilder, get_created_receipts, get_logs};
use near_sdk::{NearToken, PromiseResult, testing_env};

pub const CONTRACT: &str = "identity-registry.erc-8004.testnet";
pub const ALICE: &str = "alice.testnet";
pub const BOB: &str = "bob.testnet";
pub const CAROL: &str = "carol.testnet";
/// Una cuenta aprobada (NEP-178) por el dueno: el "operador" de ERC-8004.
pub const OP: &str = "operador.testnet";
pub const WALLET: &str = "wallet.testnet";
pub const MARKET: &str = "market.testnet";
/// 2026-11-01T00:00Z.
pub const NOW: u64 = 1_793_491_200_000_000_000;
pub const URI: &str = "https://agente.example/registration.json";
pub const BYTE: u128 = 10_000_000_000_000_000_000;
pub const NEAR: u128 = 1_000_000_000_000_000_000_000_000;
// El mock de near-sdk arranca `env::storage_usage()` en el valor del `VMContext` de cada `testing_env!`: los contextos
// de aca le pasan el del contexto anterior, asi el uso acumulado se puede medir entre llamadas (con `flush_all` antes).

pub fn acc(s: &str) -> AccountId {
    s.parse().unwrap()
}

pub fn ctx_full(predecessor: &str, signer: &str, deposit: u128, ts: u64, gas: Gas) {
    let usage = env::storage_usage();
    testing_env!(
        VMContextBuilder::new()
            .storage_usage(usage)
            .current_account_id(acc(CONTRACT))
            .predecessor_account_id(acc(predecessor))
            .signer_account_id(acc(signer))
            .attached_deposit(NearToken::from_yoctonear(deposit))
            .account_balance(NearToken::from_near(10))
            .block_timestamp(ts)
            .prepaid_gas(gas)
            .build()
    );
}

pub fn ctx(predecessor: &str, deposit: u128) {
    ctx_full(predecessor, predecessor, deposit, NOW, Gas::from_tgas(300));
}

pub fn ctx_at(predecessor: &str, deposit: u128, ts: u64) {
    ctx_full(predecessor, predecessor, deposit, ts, Gas::from_tgas(300));
}

/// Contexto de `nft_resolve_transfer`: predecessor = el contrato y `r` como resultado de `nft_on_transfer`.
pub fn ctx_callback(r: PromiseResult) {
    let usage = env::storage_usage();
    testing_env!(
        VMContextBuilder::new()
            .storage_usage(usage)
            .current_account_id(acc(CONTRACT))
            .predecessor_account_id(acc(CONTRACT))
            .signer_account_id(acc(ALICE))
            .account_balance(NearToken::from_near(10))
            .block_timestamp(NOW + 1_000_000_000)
            .build(),
        near_sdk::test_vm_config(),
        near_sdk::RuntimeFeesConfig::test(),
        Default::default(),
        vec![r]
    );
}

pub fn new_contract() -> IdentityRegistry {
    ctx(CONTRACT, 0);
    IdentityRegistry::new()
}

/// `register()` de `who` con 1 NEAR (vuelve el sobrante).
pub fn reg(c: &mut IdentityRegistry, who: &str) -> String {
    ctx(who, NEAR);
    c.register(None, None)
}

pub fn reg_uri(c: &mut IdentityRegistry, who: &str, uri: &str) -> String {
    ctx(who, NEAR);
    c.register(Some(uri.into()), None)
}

/// `nft_approve` de `owner` a `who` (con 1 NEAR).
pub fn approve(c: &mut IdentityRegistry, owner: &str, id: &str, who: &str) {
    ctx(owner, NEAR);
    assert!(c.nft_approve(id.into(), acc(who), None).is_none());
}

pub fn transfer(c: &mut IdentityRegistry, from: &str, to: &str, id: &str) {
    ctx(from, 1);
    c.nft_transfer(acc(to), id.into(), None, None);
}

/// Las transferencias de las promesas creadas: (receptor, yocto).
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

/// Lo que se le devolvio a `who` en el ultimo contexto.
pub fn refunded_to(who: &str) -> u128 {
    transfers()
        .into_iter()
        .filter(|(r, _)| r == who)
        .map(|(_, y)| y)
        .sum()
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

/// Los eventos (`standard`, `event`, `data[0]`) del ultimo contexto, en orden.
pub fn events() -> Vec<(String, String, Value)> {
    get_logs()
        .iter()
        .filter_map(|l| l.strip_prefix("EVENT_JSON:"))
        .map(|j| {
            let e: Value = serde_json::from_str(j).unwrap();
            assert_eq!(e["data"].as_array().unwrap().len(), 1);
            (
                e["standard"].as_str().unwrap().to_string(),
                e["event"].as_str().unwrap().to_string(),
                e["data"][0].clone(),
            )
        })
        .collect()
}

pub fn event_names() -> Vec<String> {
    events().into_iter().map(|e| e.1).collect()
}

pub fn logs() -> Vec<String> {
    get_logs()
}

pub fn b64(s: &[u8]) -> String {
    near_sdk::base64::Engine::encode(&near_sdk::base64::engine::general_purpose::STANDARD, s)
}
