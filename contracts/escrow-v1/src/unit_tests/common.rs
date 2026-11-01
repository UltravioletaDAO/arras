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

/// Contexto de un callback: predecessor = el contrato y `r` como resultado de la promesa anterior (el
/// `ft_transfer`), como lo entrega el runtime.
pub fn ctx_callback(r: near_sdk::PromiseResult, ts: u64) {
    testing_env!(
        VMContextBuilder::new()
            .current_account_id(acc(CONTRACT))
            .predecessor_account_id(acc(CONTRACT))
            .signer_account_id(acc(OTHER))
            .account_balance(near(10))
            .block_timestamp(ts)
            .build(),
        near_sdk::test_vm_config(),
        near_sdk::RuntimeFeesConfig::test(),
        Default::default(),
        vec![r]
    );
}

/// `on_withdraw` con el resultado del `ft_transfer`: `true` = `Successful` vacio (lo que devuelve `ft_transfer`),
/// `false` = `Failed`.
pub fn on_withdraw_con(c: &mut Arras, account: &str, amount: u128, ok: bool, ts: u64) -> u128 {
    let r = if ok {
        near_sdk::PromiseResult::Successful(vec![])
    } else {
        near_sdk::PromiseResult::Failed
    };
    ctx_callback(r, ts);
    c.on_withdraw(acc(account), U128(amount)).0
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

/// Lo que un rechazo no puede tocar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub payments: u32,
    pub tvl_global: u128,
    pub in_flight: u128,
    pub tvl_operador: u128,
    pub payer_tvl: u128,
    pub sponsor_pool: u128,
    pub sponsored_used: u128,
    pub storage_payer: Option<(u128, u128)>,
    pub storage_available_total: u128,
    pub claimable: [u128; 3],
    pub storage_usage: u64,
}

pub fn snapshot(c: &mut Arras, payer: &str) -> Snapshot {
    c.flush_all();
    let op = c.operators.get(OP).cloned();
    let payer = acc(payer);
    Snapshot {
        payments: c.payments.len(),
        tvl_global: c.tvl_global,
        in_flight: c.in_flight,
        tvl_operador: op.as_ref().map_or(0, |o| o.tvl_operador.0),
        payer_tvl: c.payer_tvl_of(OP, &payer),
        sponsor_pool: op.as_ref().map_or(0, |o| o.sponsor_pool.0),
        sponsored_used: c.sponsored_used_of(OP, &payer),
        storage_payer: c.storage.get(&payer).map(|b| (b.total, b.available)),
        storage_available_total: c.storage_available_total,
        claimable: [
            c.claimable_of(&payer),
            c.claimable_of(&acc(WORKER)),
            c.claimable_of(&acc(FEES)),
        ],
        storage_usage: env::storage_usage(),
    }
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

/// El `msg` de V-1 (sec. 3.2 y 7) como struct, para variarlo.
pub fn msg_v1() -> arras_escrow_core::AuthorizeMsg {
    arras_escrow_core::AuthorizeMsg {
        operator_id: OP.into(),
        receiver: acc(WORKER),
        release_deadline: RELEASE,
        reclaim_after: RECLAIM,
        fee_bps_max: 1300,
        terms_hash: [0; 32],
        salt: [0; 16],
    }
}

/// `msg` de V-1 con otro `salt` (para ids distintos).
pub fn msg_salt(n: u8) -> String {
    let mut m = msg_v1();
    m.salt[15] = n;
    m.to_canonical_json()
}

/// `ft_on_transfer` en el contexto actual (sin cambiarlo: `testing_env!` reinicia el contador de
/// `env::storage_usage()`, asi que las mediciones van dentro de un mismo contexto).
pub fn call_ft_on_transfer(c: &mut Arras, sender: &str, amount: u128, msg: &str) -> u128 {
    use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;
    match c.ft_on_transfer(acc(sender), U128(amount), msg.to_string()) {
        near_sdk::PromiseOrValue::Value(v) => v.0,
        near_sdk::PromiseOrValue::Promise(_) => panic!("ft_on_transfer no devuelve promesas"),
    }
}

/// `ft_on_transfer` llamado por `token` (predecessor) en `NOW`. Devuelve lo que el contrato devuelve al token.
pub fn ft_on_transfer_as(
    c: &mut Arras,
    token: &str,
    sender: &str,
    amount: u128,
    msg: &str,
) -> u128 {
    ctx(token, 0, NOW);
    call_ft_on_transfer(c, sender, amount, msg)
}

/// El `payment_id` que daria authorize.
pub fn id_of(sender: &str, amount: u128, msg: &str) -> PaymentId {
    let parsed = arras_escrow_core::parse_authorize_msg(msg).unwrap();
    arras_escrow_core::compute_payment_id(&arras_escrow_core::PaymentIdInput::from_msg(
        &parsed, USDC, sender, amount,
    ))
    .1
}

/// authorize valido desde el USDC; afirma `Value(0)` y devuelve el `payment_id`.
pub fn authorize(c: &mut Arras, sender: &str, amount: u128, msg: &str) -> PaymentId {
    let back = ft_on_transfer_as(c, USDC, sender, amount, msg);
    assert_eq!(back, 0, "authorize rechazado: {:?}", logs());
    id_of(sender, amount, msg)
}

/// Rechazo: `Value(amount)`, el log `arras:rechazo:<code>` y el estado intacto (incluidos los bytes de storage).
pub fn assert_rechazo(
    c: &mut Arras,
    token: &str,
    sender: &str,
    amount: u128,
    msg: &str,
    code: &str,
) {
    ctx(token, 0, NOW);
    let antes = snapshot(c, sender);
    let back = call_ft_on_transfer(c, sender, amount, msg);
    assert_eq!(back, amount, "el rechazo devuelve todo");
    assert_eq!(logs(), vec![format!("arras:rechazo:{code}")]);
    assert_eq!(snapshot(c, sender), antes, "estado intacto tras {code}");
}

/// Invariante de la sec. 3.5 sobre las cuentas dadas (las unicas con `claimable` en el test):
/// `sum(vivos) + sum(claimable) + in_flight == tvl_global`, y `tvl_operador`/`payer_tvl` = lo vivo de cada uno.
pub fn assert_invariante(c: &Arras, cuentas: &[&str]) {
    let vivos: u128 = c
        .payments
        .values()
        .filter(|p| p.status.is_live())
        .map(|p| p.live_amount())
        .sum();
    let claim: u128 = cuentas.iter().map(|a| c.claimable_of(&acc(a))).sum();
    assert_eq!(
        vivos + claim + c.in_flight,
        c.tvl_global,
        "sum(vivos) + sum(claimable) + in_flight == tvl_global"
    );
    for (op_id, op) in c.operators.iter() {
        let vivo_op: u128 = c
            .payments
            .values()
            .filter(|p| &p.operator_id == op_id && p.status.is_live())
            .map(|p| p.live_amount())
            .sum();
        assert_eq!(op.tvl_operador.0, vivo_op, "tvl_operador de {op_id}");
        for a in cuentas {
            let vivo_payer: u128 = c
                .payments
                .values()
                .filter(|p| &p.operator_id == op_id && p.payer == acc(a) && p.status.is_live())
                .map(|p| p.live_amount())
                .sum();
            assert_eq!(
                c.payer_tvl_of(op_id, &acc(a)),
                vivo_payer,
                "payer_tvl de ({op_id}, {a})"
            );
        }
    }
}

/// V-1 autorizado; devuelve su id.
pub fn setup_v1() -> (Arras, PaymentId) {
    let mut c = setup();
    let id = authorize(&mut c, PAYER, 1_000_000, &msg_v1().to_canonical_json());
    (c, id)
}

pub const TODAS: [&str; 6] = [PAYER, WORKER, FEES, OTHER, CAPTURER, OWNER];

/// Las transferencias de NEAR que la llamada creo: `(destino, yocto)`.
pub fn transfers() -> Vec<(String, u128)> {
    near_sdk::test_utils::get_created_receipts()
        .into_iter()
        .flat_map(|r| {
            let to = r.receiver_id.to_string();
            r.actions.into_iter().filter_map(move |a| match a {
                near_sdk::mock::MockAction::Transfer { deposit, .. } => {
                    Some((to.clone(), deposit.as_yoctonear()))
                }
                _ => None,
            })
        })
        .collect()
}
