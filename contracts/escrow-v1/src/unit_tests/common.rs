use crate::*;
use near_sdk::test_utils::{VMContextBuilder, get_logs};
use near_sdk::testing_env;

pub const USDC: &str = "3e2210e1184b45b64c8a434c0a7e7b23cc04ea7eb7a6c3c32520d03d4afcb8af";
pub const CONTRACT: &str = "v1.arras-dev.testnet";
pub const OWNER: &str = "owner.arras-dev.testnet";
pub const GUARDIAN: &str = "guardian.arras-dev.testnet";
pub const CAPTURER: &str = "em-capturer.arras-dev.testnet";
pub const FEES: &str = "em-fees.arras-dev.testnet";
/// El `sender_id` de U-03 (el de V-1).
pub const PAYER: &str = "payer-demo.testnet";
/// El receiver de V-1.
pub const WORKER: &str = "worker-demo.testnet";
pub const OTHER: &str = "tercero.testnet";
pub const OP: &str = "em";

/// 2026-11-01T00:00Z.
pub const NOW: u64 = 1_793_491_200_000_000_000;
/// 2026-11-02T00:00Z (V-1).
pub const RELEASE: u64 = 1_793_577_600_000_000_000;
/// 2026-11-03T00:00Z (V-1).
pub const RECLAIM: u64 = 1_793_664_000_000_000_000;

pub const USDC_UNIT: u128 = 1_000_000;
pub const TOPE_POR_PAGO: u128 = 10 * USDC_UNIT;
/// Decision 2 del encargo: 1.000 USDC.
pub const TOPE_TVL_GLOBAL: u128 = 1_000 * USDC_UNIT;
pub const MIN_AMOUNT: u128 = 10_000;
pub const TOPE_TVL_OPERADOR: u128 = 200 * USDC_UNIT;
pub const TOPE_TVL_POR_PAYER: u128 = 20 * USDC_UNIT;
/// 0,02 NEAR.
pub const SPONSOR_MAX: u128 = 20_000_000_000_000_000_000_000;

pub fn acc(s: &str) -> AccountId {
    s.parse().unwrap()
}

pub fn near(n: u128) -> NearToken {
    NearToken::from_near(n)
}

/// Contexto con `predecessor`, deposito en yocto y hora.
pub fn ctx(predecessor: &str, deposit_yocto: u128, ts: u64) {
    testing_env!(
        VMContextBuilder::new()
            .current_account_id(acc(CONTRACT))
            .predecessor_account_id(acc(predecessor))
            .signer_account_id(acc(predecessor))
            .attached_deposit(NearToken::from_yoctonear(deposit_yocto))
            .account_balance(near(10))
            .block_timestamp(ts)
            .build()
    );
}

pub fn new_contract() -> Arras {
    ctx(CONTRACT, 0, NOW);
    Arras::new(
        acc(OWNER),
        acc(GUARDIAN),
        acc(USDC),
        U128(TOPE_POR_PAGO),
        U128(TOPE_TVL_GLOBAL),
    )
}

pub fn operator_input() -> OperatorInput {
    OperatorInput {
        owner: acc(OWNER),
        capturers: vec![acc(CAPTURER)],
        fee_bps: 1300,
        fee_recipient: acc(FEES),
        min_amount: U128(MIN_AMOUNT),
        tope_tvl_operador: U128(TOPE_TVL_OPERADOR),
        tope_tvl_por_payer: U128(TOPE_TVL_POR_PAYER),
        sponsor_max_por_payer: U128(SPONSOR_MAX),
    }
}

/// Contrato con el operador `em` de la sec. 8 y el payer con 0,05 NEAR de NEP-145.
pub fn setup() -> Arras {
    let mut c = new_contract();
    ctx(OWNER, 1, NOW);
    c.register_operator(OP.into(), operator_input());
    ctx(PAYER, NearToken::from_millinear(50).as_yoctonear(), NOW);
    c.storage_deposit(None, None);
    ctx(OWNER, 0, NOW);
    c
}

/// El panic de `f` contiene `needle` (p. ej. `ARRAS_E21`).
pub fn expect_panic<R>(needle: &str, f: impl FnOnce() -> R) {
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    let err = match r {
        Ok(_) => panic!("se esperaba un panic con {needle}"),
        Err(e) => e,
    };
    let msg = err
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| err.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_default();
    assert!(msg.contains(needle), "panic {msg:?} no contiene {needle:?}");
}

/// `(total, available)` de un `StorageBalance` (no tiene `PartialEq`).
pub fn sb(b: near_contract_standards::storage_management::StorageBalance) -> (u128, u128) {
    (b.total.as_yoctonear(), b.available.as_yoctonear())
}

pub fn logs() -> Vec<String> {
    get_logs()
}
