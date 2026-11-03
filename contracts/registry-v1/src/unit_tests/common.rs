use crate::*;
use near_sdk::mock::MockAction;
use near_sdk::serde_json::{self, Value, json};
use near_sdk::test_utils::{VMContextBuilder, get_created_receipts, get_logs};
use near_sdk::{PromiseResult, testing_env};

pub const CONTRACT: &str = "registry.arras-dev.testnet";
pub const ESCROW: &str = "v1.arras-dev.testnet";
pub const ESCROW2: &str = "v2.arras-dev.testnet";
pub const FUERA: &str = "falso-escrow.testnet";
pub const PAYER: &str = "agente.testnet";
pub const WORKER: &str = "worker.testnet";
pub const OTHER: &str = "tercero.testnet";
/// El `payment_id` de V-1.
pub const PID: &str = "DN5eMA3sNES7xsoy6qvy24cVF6Dmwjxpkvd2b4e8P38M";
pub const PID2: &str = "2pGs8e5CXCrrz3cMQS9Tfu4gKV8vGWq6cEQTuALG5TfP";
/// 2026-11-01T00:00Z.
pub const NOW: u64 = 1_793_491_200_000_000_000;
pub const HEIGHT: u64 = 1_000_000;
pub const URI: &str = "https://agente.example/card.json";
pub const HASH: &str = "00ff10a0b1c2d3e4f5061728394a5b6c7d8e9fa0b1c2d3e4f5061728394a5b6c";
/// La entrada del candado (clave `p` + 32 bytes, valor u64): la escribe `give_feedback` y la borra el callback.
pub const PENDING_RECORD_BYTES: u64 = record_bytes(1 + 32, 8);
// Ojo al medir storage en estos tests: el mock de near-sdk vuelve `env::storage_usage()` a su valor inicial en cada
// `testing_env!`; se mide antes y despues dentro del mismo contexto.

pub fn acc(s: &str) -> AccountId {
    s.parse().unwrap()
}

pub fn ctx_full(predecessor: &str, deposit: u128, ts: u64, height: u64, gas: Gas) {
    testing_env!(
        VMContextBuilder::new()
            .current_account_id(acc(CONTRACT))
            .predecessor_account_id(acc(predecessor))
            .signer_account_id(acc(predecessor))
            .attached_deposit(NearToken::from_yoctonear(deposit))
            .account_balance(NearToken::from_near(10))
            .block_timestamp(ts)
            .block_height(height)
            .prepaid_gas(gas)
            .build()
    );
}

/// Contexto de una llamada de `predecessor` con `deposit` yocto, a `NOW` y `HEIGHT`, con 300 TGas.
pub fn ctx(predecessor: &str, deposit: u128) {
    ctx_full(predecessor, deposit, NOW, HEIGHT, Gas::from_tgas(300));
}

pub fn ctx_at(predecessor: &str, deposit: u128, height: u64) {
    ctx_full(predecessor, deposit, NOW, height, Gas::from_tgas(300));
}

/// Contexto del callback: predecessor = el contrato y `r` como resultado de `get_payment`.
pub fn ctx_callback(r: PromiseResult, height: u64) {
    testing_env!(
        VMContextBuilder::new()
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
        vec![r]
    );
}

pub fn new_with(escrows: &[&str]) -> Registry {
    ctx(CONTRACT, 0);
    Registry::new(escrows.iter().map(|e| acc(e)).collect())
}

pub fn new_contract() -> Registry {
    new_with(&[ESCROW, ESCROW2])
}

pub fn deposit() -> u128 {
    Registry::feedback_deposit()
}

/// Un `Payment` de Arras v1 tal como lo devuelve `get_payment` (todos los campos).
pub fn payment_v1(payer: &str, receiver: &str, status: &str, released: u128) -> Value {
    json!({
        "operator_id": "em",
        "payer": payer,
        "receiver": receiver,
        "amount": "1000000",
        "released": released.to_string(),
        "refunded": "0",
        "fee_bps": 1300,
        "fee_recipient": "em-fees.arras-dev.testnet",
        "fee_bps_max": 1800,
        "created_at": "1793491200000000000",
        "release_deadline": "1793577600000000000",
        "reclaim_after": "1793664000000000000",
        "terms_hash": "77a2c0d6b3e9c3c5a6a7b48e1e8f6d7c0b6d1f6e4d0a7c2b9e8f7a6b5c4d3e2f",
        "status": status,
        "storage_charged": "3770000000000000000000",
        "sponsored": false
    })
}

pub fn released_json() -> String {
    payment_v1(PAYER, WORKER, "Released", 1_000_000).to_string()
}

pub fn ok_bytes(s: &str) -> PromiseResult {
    PromiseResult::Successful(s.as_bytes().to_vec())
}

#[derive(Clone)]
pub struct Pedido {
    pub escrow: String,
    pub payment_id: String,
    pub score: u8,
    pub tag1: Option<String>,
    pub tag2: Option<String>,
    pub feedback_uri: Option<String>,
    pub feedback_hash: Option<String>,
}

impl Default for Pedido {
    fn default() -> Self {
        Self {
            escrow: ESCROW.into(),
            payment_id: PID.into(),
            score: 90,
            tag1: Some("quality".into()),
            tag2: Some("successRate".into()),
            feedback_uri: Some("https://em.example/feedback/1.json".into()),
            feedback_hash: Some(HASH.into()),
        }
    }
}

/// Llama `give_feedback` sin mirar las promesas.
pub fn call_give(c: &mut Registry, p: &Pedido) {
    c.give_feedback(
        acc(&p.escrow),
        p.payment_id.clone(),
        p.score,
        p.tag1.clone(),
        p.tag2.clone(),
        p.feedback_uri.clone(),
        p.feedback_hash.clone(),
    )
    .detach();
}

/// `give_feedback` de `rater` con `deposit` a la altura `height`; devuelve los argumentos del callback tal como
/// quedaron en la promesa (asi el test usa exactamente lo que el contrato mando).
pub fn give_at(
    c: &mut Registry,
    rater: &str,
    dep: u128,
    height: u64,
    p: &Pedido,
) -> PendingFeedback {
    ctx_at(rater, dep, height);
    call_give(c, p);
    callback_args()
}

pub fn give(c: &mut Registry, rater: &str, p: &Pedido) -> PendingFeedback {
    give_at(c, rater, deposit(), HEIGHT, p)
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

pub fn callback_args() -> PendingFeedback {
    let call = function_calls()
        .into_iter()
        .find(|c| c.1 == "on_payment_checked")
        .expect("give_feedback no creo el callback");
    serde_json::from_value(call.2["p"].clone()).unwrap()
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

/// Corre el callback con `r` como resultado de `get_payment`, a `HEIGHT + 2`.
pub fn callback(c: &mut Registry, p: PendingFeedback, r: PromiseResult) -> Option<u32> {
    ctx_callback(r, HEIGHT + 2);
    c.on_payment_checked(p)
}

/// give + callback con el pago dado. Devuelve el resultado del callback.
pub fn rate(c: &mut Registry, rater: &str, p: &Pedido, payment: &str) -> Option<u32> {
    let args = give(c, rater, p);
    callback(c, args, ok_bytes(payment))
}

/// Los eventos `EVENT_JSON` del registro con ese nombre, en el ultimo contexto.
pub fn events(name: &str) -> Vec<Value> {
    get_logs()
        .iter()
        .filter_map(|l| l.strip_prefix("EVENT_JSON:"))
        .map(|j| serde_json::from_str::<Value>(j).unwrap())
        .filter(|e| e["standard"] == EVENT_STANDARD && e["event"] == name)
        .collect()
}

pub fn logs() -> Vec<String> {
    get_logs()
}

/// Rechazo del callback: devuelve `None`, deja el log con el codigo, devuelve el deposito entero y libera el candado.
pub fn assert_rechazo(c: &mut Registry, r: Option<u32>, rater: &str, p: &Pedido, e: RegistryError) {
    assert_eq!(r, None, "{e:?}");
    assert!(
        logs().contains(&e.rejection_log()),
        "{:?} sin {e:?}",
        logs()
    );
    assert_eq!(transfers(), vec![(rater.to_string(), deposit())]);
    assert!(events("feedback_given").is_empty());
    assert!(
        !c.is_feedback_pending(acc(&p.escrow), p.payment_id.clone(), acc(rater)),
        "el candado quedo puesto"
    );
    assert_eq!(
        c.get_feedback_for_payment(acc(&p.escrow), p.payment_id.clone(), acc(rater)),
        None
    );
}
