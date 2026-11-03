use crate::*;
use near_sdk::test_utils::{VMContextBuilder, get_logs};
use near_sdk::testing_env;

pub const USDC: &str = "3e2210e1184b45b64c8a434c0a7e7b23cc04ea7eb7a6c3c32520d03d4afcb8af";
pub const CONTRACT: &str = "v2.arras-dev.testnet";
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
/// El arbitro del operador `em` (sec. 1 de la prescripcion).
pub const ARBITER: &str = "arbiter.arras-dev.testnet";
/// El agente del presupuesto de PAYER (el humano).
pub const AGENT: &str = "agent-demo.testnet";
/// Plazo del arbitro de los tests: 3 dias.
pub const ARBITER_WINDOW: u64 = 3 * DAY_NS;

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

/// Contrato nuevo sobre un storage mockeado vacio (un test puede armar varios).
pub fn new_contract() -> Arras {
    let _ = near_sdk::mock::with_mocked_blockchain(|b| b.take_storage());
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
        arbiter: acc(ARBITER),
        arbiter_window_ns: U64(ARBITER_WINDOW),
        ticket_pubkey: None,
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
    pub budgets_total: u128,
    pub budget: Option<Budget>,
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
        budgets_total: c.budgets_total,
        budget: c.budget_of(payer.as_str(), AGENT, OP),
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

/// El `payment_id` que daria authorize (con un `msg` de v1 o de v2).
pub fn id_of(sender: &str, amount: u128, msg: &str) -> PaymentId {
    let parsed = crate::msg::parse_authorize_msg_v2(msg).unwrap();
    arras_escrow_core::compute_payment_id(&arras_escrow_core::PaymentIdInput::from_msg(
        &parsed.base,
        USDC,
        sender,
        amount,
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

/// Agentes que usan los tests (para sumar presupuestos: un `LookupMap` no se recorre).
pub const AGENTES: [&str; 2] = [AGENT, "agent-dos.testnet"];

/// Suma de los presupuestos de `owner` (o de todos los de `cuentas` si `owner` es `None`) en `op`.
pub fn budgets_de(c: &Arras, cuentas: &[&str], owner: Option<&str>, op: &str) -> u128 {
    let mut total = 0;
    for o in cuentas {
        if owner.is_some_and(|x| x != *o) {
            continue;
        }
        for a in AGENTES {
            total += c.budget_of(o, a, op).map_or(0, |b| b.saldo.0);
        }
    }
    total
}

/// Invariante de v2 (sec. 3.5 con presupuestos) sobre las cuentas dadas (las unicas con `claimable` o presupuesto en
/// el test): `sum(en escrow) + sum(claimable) + in_flight + sum(presupuestos) == tvl_global`, `budgets_total` igual a
/// la suma, y `tvl_operador`/`payer_tvl` = lo que sigue en escrow mas los presupuestos de cada uno. "En escrow"
/// incluye lo congelado en disputa.
pub fn assert_invariante(c: &Arras, cuentas: &[&str]) {
    let en_escrow: u128 = c.payments.values().map(|p| p.live_amount()).sum();
    let claim: u128 = cuentas.iter().map(|a| c.claimable_of(&acc(a))).sum();
    let ops: Vec<String> = c.operators.keys().cloned().collect();
    let budgets: u128 = ops.iter().map(|o| budgets_de(c, cuentas, None, o)).sum();
    assert_eq!(
        budgets, c.budgets_total,
        "budgets_total == sum(presupuestos)"
    );
    let reserva: u128 = c
        .payments
        .values()
        .map(|p| match &p.dispute {
            None => bytes_cost(DISPUTE_RESERVE_BYTES),
            Some(d) if d.resolved_at.is_none() => bytes_cost(DISPUTE_RESOLVE_BYTES),
            Some(_) => 0,
        })
        .sum();
    assert_eq!(
        c.dispute_reserve_total, reserva,
        "dispute_reserve_total == 86 bytes por pago sin disputa + 8 por disputa abierta"
    );
    assert_eq!(
        en_escrow + claim + c.in_flight + budgets,
        c.tvl_global,
        "sum(en escrow) + sum(claimable) + in_flight + sum(presupuestos) == tvl_global"
    );
    for (op_id, op) in c.operators.iter() {
        let vivo_op: u128 = c
            .payments
            .values()
            .filter(|p| &p.operator_id == op_id)
            .map(|p| p.live_amount())
            .sum();
        assert_eq!(
            op.tvl_operador.0,
            vivo_op + budgets_de(c, cuentas, None, op_id),
            "tvl_operador de {op_id}"
        );
        for a in cuentas {
            let vivo_payer: u128 = c
                .payments
                .values()
                .filter(|p| &p.operator_id == op_id && p.payer == acc(a))
                .map(|p| p.live_amount())
                .sum();
            assert_eq!(
                c.payer_tvl_of(op_id, &acc(a)),
                vivo_payer + budgets_de(c, cuentas, Some(a), op_id),
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

/// El `msg` `budget` de la sec. 4 para `agent`, con topes.
pub fn budget_msg(agent: &str, tope_por_pago: u128, tope_diario: u128) -> crate::msg::BudgetMsg {
    crate::msg::BudgetMsg {
        operator_id: OP.into(),
        agent: acc(agent),
        tope_por_pago,
        tope_diario,
        operator_ticket: None,
    }
}

/// Deposito de presupuesto valido desde el USDC en `NOW`; afirma `Value(0)`.
pub fn deposit(c: &mut Arras, owner: &str, agent: &str, amount: u128, topes: (u128, u128)) {
    let m = budget_msg(agent, topes.0, topes.1).to_canonical_json();
    let back = ft_on_transfer_as(c, USDC, owner, amount, &m);
    assert_eq!(back, 0, "deposito rechazado: {:?}", logs());
}

/// Los argumentos de `authorize_from_budget` (todos menos `owner`), para variarlos.
#[derive(Clone, Debug)]
pub struct Afb {
    pub operator_id: String,
    pub receiver: String,
    pub amount: u128,
    pub release_deadline: u64,
    pub reclaim_after: u64,
    pub dispute_deadline: Option<u64>,
    pub fee_bps_max: u16,
    pub terms_hash: String,
    pub salt: String,
    pub ticket: Option<TicketArg>,
}

/// Un pago de 1 USDC a WORKER con los plazos de V-1 y el salt `n`.
pub fn afb(amount: u128, n: u8) -> Afb {
    let mut salt = [0u8; 16];
    salt[15] = n;
    Afb {
        operator_id: OP.into(),
        receiver: WORKER.into(),
        amount,
        release_deadline: RELEASE,
        reclaim_after: RECLAIM,
        dispute_deadline: None,
        fee_bps_max: 1300,
        terms_hash: "0".repeat(64),
        salt: crate::msg::hex_encode(&salt),
        ticket: None,
    }
}

/// `authorize_from_budget` llamado por `agent` en el contexto actual.
pub fn call_afb(c: &mut Arras, owner: &str, a: &Afb) -> PaymentId {
    c.authorize_from_budget(
        acc(owner),
        a.operator_id.clone(),
        acc(&a.receiver),
        U128(a.amount),
        U64(a.release_deadline),
        U64(a.reclaim_after),
        a.dispute_deadline.map(U64),
        a.fee_bps_max,
        a.terms_hash.clone(),
        a.salt.clone(),
        a.ticket.clone(),
    )
}

/// El `payment_id` de un `authorize_from_budget` (sec. 7 con `payer = owner`).
pub fn afb_id(owner: &str, a: &Afb) -> PaymentId {
    let m = arras_escrow_core::AuthorizeMsg {
        operator_id: a.operator_id.clone(),
        receiver: acc(&a.receiver),
        release_deadline: a.release_deadline,
        reclaim_after: a.reclaim_after,
        fee_bps_max: a.fee_bps_max,
        terms_hash: crate::msg::parse_hex_32(&a.terms_hash).unwrap(),
        salt: crate::msg::parse_hex_16(&a.salt).unwrap(),
    };
    arras_escrow_core::compute_payment_id(&arras_escrow_core::PaymentIdInput::from_msg(
        &m, USDC, owner, a.amount,
    ))
    .1
}

/// Llave de tickets de los tests de contrato: semilla `02` x 32 (la de los vectores, `01` x 32, el contrato la rechaza
/// como `ticket_pubkey` porque su semilla esta publicada).
pub fn ticket_signing_key() -> ed25519_dalek::SigningKey {
    ed25519_dalek::SigningKey::from_bytes(&[2; 32])
}

/// `ticket_pubkey` de la llave de los tests.
pub fn ticket_pubkey() -> PublicKey {
    PublicKey::from_parts(
        near_sdk::CurveType::ED25519,
        ticket_signing_key().verifying_key().to_bytes().to_vec(),
    )
    .unwrap()
}

/// Firma un ticket para `subject` con la llave de los tests, sobre el contrato de los tests.
pub fn firmar_ticket(subject: crate::ticket::TicketSubject<'_>, expiry: u64) -> TicketArg {
    use ed25519_dalek::Signer;
    let m = crate::ticket::ticket_message(CONTRACT, subject, expiry);
    TicketArg {
        expiry: U64(expiry),
        sig: near_sdk::bs58::encode(ticket_signing_key().sign(&m).to_bytes()).into_string(),
    }
}

/// Le pone la llave de tickets al operador `em`.
pub fn con_ticket(c: &mut Arras) {
    ctx(OWNER, 1, NOW);
    c.update_operator(
        OP.into(),
        OperatorPatch {
            ticket_pubkey: Some(ticket_pubkey()),
            ..Default::default()
        },
    );
}

/// `msg` `authorize` de v2 a partir de un `msg` de v1 con `dispute_deadline` y ticket opcionales.
pub fn msg_v2(
    base: arras_escrow_core::AuthorizeMsg,
    dispute_deadline: Option<u64>,
    ticket: Option<&TicketArg>,
) -> String {
    crate::msg::AuthorizeMsgV2 {
        base,
        dispute_deadline,
        operator_ticket: ticket.map(|t| crate::msg::OperatorTicket {
            expiry: t.expiry.0,
            sig: t.sig.clone(),
        }),
    }
    .to_canonical_json()
}

/// El plan de un `ft_on_transfer` (sin escribir), para leer el costo de storage antes de autorizar.
pub fn plan_cost(c: &Arras, sender: &str, amount: u128, msg: &str) -> u128 {
    match c.plan_ft(&acc(sender), amount, msg) {
        Ok(crate::authorize::FtPlan::Authorize(p)) => p.payment.storage_charged.0,
        Ok(crate::authorize::FtPlan::Budget(p)) => p.budget.storage_charged.0,
        Err(e) => panic!("plan rechazado: {e:?}"),
    }
}
