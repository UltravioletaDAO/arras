//! C3: estado, `new`, NEP-145 y vistas. U-24 (`new` ajeno) corre contra la wasm real en
//! `tests/workspaces.rs`: `#[private]` solo existe en el wrapper de la wasm.

use super::common::*;
use crate::storage::STORAGE_RECORD_MAX_BYTES;
use crate::*;

#[test]
fn new_deja_la_config_de_la_decision_2() {
    let c = new_contract();
    let cfg = c.get_config();
    assert_eq!(cfg.version, "arras-v1");
    assert_eq!(cfg.owner, acc(OWNER));
    assert_eq!(cfg.guardian, acc(GUARDIAN));
    assert_eq!(cfg.token, acc(USDC));
    assert_eq!(cfg.tope_por_pago, U128(10_000_000));
    assert_eq!(cfg.tope_tvl_global, U128(1_000_000_000));
    assert!(!cfg.paused);
    assert_eq!(
        (cfg.tvl_global, cfg.in_flight, cfg.payments, cfg.operators),
        (U128(0), U128(0), 0, 0)
    );
}

#[test]
fn storage_bounds_cubren_el_registro_de_una_cuenta_de_64() {
    let c = new_contract();
    let b = c.storage_balance_bounds();
    assert_eq!(STORAGE_RECORD_MAX_BYTES, 40 + 1 + 4 + 64 + 32);
    assert_eq!(b.min.as_yoctonear(), 141 * 10u128.pow(19));
    assert_eq!(b.max, None);
}

#[test]
fn storage_deposit_registro_y_recarga() {
    let mut c = new_contract();
    let min = c.storage_balance_bounds().min.as_yoctonear();
    ctx(PAYER, min - 1, NOW);
    expect_panic("less than the minimum storage balance", || {
        c.storage_deposit(None, None)
    });

    ctx(PAYER, NearToken::from_millinear(50).as_yoctonear(), NOW);
    let b = sb(c.storage_deposit(None, None));
    assert_eq!(
        b,
        (
            NearToken::from_millinear(50).as_yoctonear(),
            NearToken::from_millinear(50).as_yoctonear() - min
        )
    );
    assert_eq!(c.storage_available_total, b.1);

    // Recarga para otra cuenta y registro solo: lo de mas se devuelve.
    ctx(OTHER, 2 * min, NOW);
    let w = sb(c.storage_deposit(Some(acc(WORKER)), Some(true)));
    assert_eq!(w, (min, 0));
    ctx(OTHER, 5, NOW);
    assert_eq!(sb(c.storage_deposit(Some(acc(WORKER)), Some(true))), w);
    ctx(PAYER, 7, NOW);
    let b2 = sb(c.storage_deposit(None, None));
    assert_eq!(b2, (b.0 + 7, b.1 + 7));
    assert_eq!(c.storage_balance_of(acc(PAYER)).map(sb), Some(b2));
    assert!(c.storage_balance_of(acc(OTHER)).is_none());
    assert_eq!(c.storage_available_total, b2.1);
}

#[test]
fn storage_withdraw_hasta_available_con_un_yocto() {
    let mut c = setup();
    let avail = c
        .storage_balance_of(acc(PAYER))
        .unwrap()
        .available
        .as_yoctonear();
    ctx(PAYER, 0, NOW);
    expect_panic("Requires attached deposit of exactly 1 yoctoNEAR", || {
        c.storage_withdraw(None)
    });
    ctx(PAYER, 1, NOW);
    expect_panic("greater than the available", || {
        c.storage_withdraw(Some(U128(avail + 1)))
    });
    ctx(OTHER, 1, NOW);
    expect_panic("is not registered", || c.storage_withdraw(None));
    ctx(PAYER, 1, NOW);
    let total = c
        .storage_balance_of(acc(PAYER))
        .unwrap()
        .total
        .as_yoctonear();
    assert_eq!(
        sb(c.storage_withdraw(Some(U128(10)))),
        (total - 10, avail - 10)
    );
    assert_eq!(sb(c.storage_withdraw(None)), (total - avail, 0));
    assert_eq!(c.storage_available_total, 0);
}

#[test]
fn register_operator_y_vistas() {
    let mut c = setup();
    let op = c.get_operator(OP.into()).unwrap();
    assert_eq!(op.fee_bps, 1300);
    assert_eq!(op.capturers, vec![acc(CAPTURER)]);
    assert_eq!((op.tvl_operador, op.sponsor_pool), (U128(0), U128(0)));
    assert_eq!(c.get_operator("kk".into()), None);
    assert_eq!(c.get_claimable(acc(WORKER)), U128(0));
    assert_eq!(c.list_payments(OP.into(), 0, 50), vec![]);
    assert_eq!(c.list_payments("kk".into(), 0, 50), vec![]);
    assert_eq!(
        c.get_payment("DN5eMA3sNES7xsoy6qvy24cVF6Dmwjxpkvd2b4e8P38M".into()),
        None
    );

    ctx(OTHER, NearToken::from_millinear(500).as_yoctonear(), NOW);
    c.fund_operator_storage(OP.into());
    assert_eq!(
        c.get_operator(OP.into()).unwrap().sponsor_pool,
        U128(NearToken::from_millinear(500).as_yoctonear())
    );
    expect_panic("ARRAS_E04", || c.fund_operator_storage("kk".into()));
}

#[test]
fn register_operator_evento_y_errores() {
    let mut c = new_contract();
    ctx(OWNER, 1, NOW);
    c.register_operator(OP.into(), operator_input());
    let l = logs();
    assert_eq!(l.len(), 1);
    assert!(l[0].starts_with(r#"EVENT_JSON:{"standard":"arras","version":"1.0.0","event":"operator_registered","data":[{"operator_id":"em","owner":"owner.arras-dev.testnet""#), "{}", l[0]);
    expect_panic("ARRAS_E44", || {
        c.register_operator(OP.into(), operator_input())
    });
    for malo in ["", "EM", "e.m", "x".repeat(33).as_str()] {
        expect_panic("ARRAS_E45", || {
            c.register_operator(malo.into(), operator_input())
        });
    }
    let mut caro = operator_input();
    caro.fee_bps = 1801;
    expect_panic("ARRAS_E43", || {
        c.register_operator("kk".into(), caro.clone())
    });
    caro.fee_bps = 1800;
    c.register_operator("kk".into(), caro);
}

#[test]
fn compute_payment_id_view_es_el_vector_v1() {
    let c = new_contract();
    let msg = format!(
        "{{\"v\":1,\"action\":\"authorize\",\"operator_id\":\"em\",\"receiver\":\"worker-demo.testnet\",\"release_deadline\":\"{RELEASE}\",\"reclaim_after\":\"{RECLAIM}\",\"fee_bps_max\":1300,\"terms_hash\":\"{}\",\"salt\":\"{}\"}}",
        "0".repeat(64),
        "0".repeat(32)
    );
    assert_eq!(
        c.compute_payment_id(acc(PAYER), U128(1_000_000), msg),
        "DN5eMA3sNES7xsoy6qvy24cVF6Dmwjxpkvd2b4e8P38M"
    );
    expect_panic("ARRAS_E03", || {
        c.compute_payment_id(acc(PAYER), U128(1), "{}".into())
    });
}

#[test]
fn storage_health_reporta_la_reserva() {
    let mut c = setup();
    ctx(OTHER, 0, NOW);
    let h = c.get_storage_health();
    assert_eq!(h.balance, U128(near(10).as_yoctonear()));
    assert_eq!(h.nep145_available.0, c.storage_available_total);
    assert_eq!(
        h.free_reserve.0,
        h.balance.0 - h.storage_cost.0 - h.nep145_available.0 - h.sponsor_pools.0
    );
    assert!(h.ok);
    ctx(OTHER, NearToken::from_millinear(500).as_yoctonear(), NOW);
    c.fund_operator_storage(OP.into());
    let h2 = c.get_storage_health();
    assert_eq!(
        h2.sponsor_pools,
        U128(NearToken::from_millinear(500).as_yoctonear())
    );
    assert_eq!(
        h2.min_free_reserve,
        U128(NearToken::from_millinear(200).as_yoctonear())
    );
}

/// El `min` de NEP-145 es el registro exacto de una cuenta de 64 caracteres: mide los bytes reales del mock
/// (overhead de 40 por registro + clave + valor), la misma cuenta que usa `ft_on_transfer` para `E14`.
#[test]
fn registro_nep145_de_64_ocupa_exactamente_min() {
    let mut c = new_contract();
    let larga = format!("{}.testnet", "a".repeat(56));
    assert_eq!(larga.len(), 64);
    let min = c.storage_balance_bounds().min.as_yoctonear();
    ctx(&larga, min, NOW);
    c.flush_all();
    let antes = env::storage_usage();
    c.storage_deposit(None, Some(true));
    c.flush_all();
    assert_eq!(env::storage_usage() - antes, STORAGE_RECORD_MAX_BYTES);
}

/// P2-4 de la refutacion: el NEAR que NEP-145 devuelve o retira, no solo el `StorageBalance` que se informa.
#[test]
fn nep145_transfiere_los_montos_exactos() {
    let mut c = new_contract();
    let min = c.storage_balance_bounds().min.as_yoctonear();
    // Registro solo: devuelve deposito - min a quien pago.
    ctx(OTHER, 3 * min, NOW);
    c.storage_deposit(Some(acc(WORKER)), Some(true));
    assert_eq!(transfers(), vec![(OTHER.to_string(), 2 * min)]);
    // Ya registrado y registro solo: devuelve todo.
    ctx(OTHER, 7, NOW);
    c.storage_deposit(Some(acc(WORKER)), Some(true));
    assert_eq!(transfers(), vec![(OTHER.to_string(), 7)]);
    // Deposito sin registration_only: no devuelve nada.
    ctx(PAYER, NearToken::from_millinear(50).as_yoctonear(), NOW);
    c.storage_deposit(None, None);
    assert_eq!(transfers(), vec![]);
    // storage_withdraw: exactamente lo pedido, al titular.
    ctx(PAYER, 1, NOW);
    c.storage_withdraw(Some(U128(1_000)));
    assert_eq!(transfers(), vec![(PAYER.to_string(), 1_000)]);
    let resto = c
        .storage_balance_of(acc(PAYER))
        .unwrap()
        .available
        .as_yoctonear();
    ctx(PAYER, 1, NOW);
    c.storage_withdraw(None);
    assert_eq!(transfers(), vec![(PAYER.to_string(), resto)]);
    ctx(PAYER, 1, NOW);
    c.storage_withdraw(None);
    assert_eq!(transfers(), vec![], "sin available no transfiere");
}

/// P3-3 de la refutacion: `free_reserve` resta los pools de patrocinio. Fondear el pool mete el NEAR en el saldo
/// y en el pool a la vez: la reserva libre no cambia (si no se restara el pool, subiria).
#[test]
fn storage_health_resta_el_pool() {
    let mut c = setup();
    let x = NearToken::from_millinear(500).as_yoctonear();
    ctx(OTHER, 0, NOW);
    let antes = c.get_storage_health();
    ctx(OTHER, x, NOW);
    c.fund_operator_storage(OP.into());
    let despues = c.get_storage_health();
    assert_eq!(despues.storage_usage, antes.storage_usage);
    assert_eq!(
        despues.balance.0,
        antes.balance.0 + x,
        "el deposito adjunto ya esta en el saldo"
    );
    assert_eq!(despues.sponsor_pools.0, antes.sponsor_pools.0 + x);
    assert_eq!(despues.free_reserve, antes.free_reserve);
    assert_eq!(
        despues.free_reserve.0,
        despues.balance.0
            - despues.storage_cost.0
            - despues.nep145_available.0
            - despues.sponsor_pools.0
    );
}

/// P3-1 de la refutacion: un `fee_recipient` igual al propio contrato es `E46` (las tarifas quedarian atrapadas).
#[test]
fn fee_recipient_no_puede_ser_el_contrato() {
    let mut c = setup();
    ctx(OWNER, 1, NOW);
    let mut i = operator_input();
    i.fee_recipient = acc(CONTRACT);
    expect_panic("ARRAS_E46", || c.register_operator("kk".into(), i.clone()));
    expect_panic("ARRAS_E46", || {
        c.update_operator(
            OP.into(),
            OperatorPatch {
                fee_recipient: Some(acc(CONTRACT)),
                ..Default::default()
            },
        )
    });
    // El orden: E43 antes que E46 en register_operator; E43 y E42 antes que E46 en update_operator (P3-D de la ronda 2).
    i.fee_bps = 1801;
    expect_panic("ARRAS_E43", || c.register_operator("kk".into(), i.clone()));
    expect_panic("ARRAS_E43", || {
        c.update_operator(
            OP.into(),
            OperatorPatch {
                fee_bps: Some(1801),
                fee_recipient: Some(acc(CONTRACT)),
                ..Default::default()
            },
        )
    });
    expect_panic("ARRAS_E42", || {
        c.update_operator(
            OP.into(),
            OperatorPatch {
                tope_tvl_operador: Some(U128(TOPE_TVL_OPERADOR + 1)),
                fee_recipient: Some(acc(CONTRACT)),
                ..Default::default()
            },
        )
    });
    assert_eq!(c.get_operator(OP.into()).unwrap().fee_recipient, acc(FEES));
    assert_eq!(c.get_operator("kk".into()), None);
}
