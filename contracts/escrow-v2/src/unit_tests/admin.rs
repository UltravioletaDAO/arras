//! C7: administracion y pausa. U-11, U-13, U-21 y U-23b (U-23 con `update_operator` real).

use super::common::*;
use crate::*;

const YOCTO_MSG: &str = "Requires attached deposit of exactly 1 yoctoNEAR";

/// U-11 `pausa`: en pausa, authorize `E02`; `release` OK; refund y reclaim OK (y withdraw y compact).
#[test]
fn u11_pausa() {
    let (mut c, id) = setup_v1();
    let id2 = authorize(&mut c, PAYER, 1_000_000, &msg_salt(2));
    let id3 = authorize(&mut c, PAYER, 1_000_000, &msg_salt(3));
    ctx(GUARDIAN, 1, NOW);
    c.pause();
    assert!(c.get_config().paused);
    assert_eq!(
        logs(),
        vec![format!(
            r#"EVENT_JSON:{{"standard":"arras","version":"1.0.0","event":"paused","data":[{{"by":"{GUARDIAN}","paused":true,"at":"{NOW}"}}]}}"#
        )]
    );
    assert_rechazo(&mut c, USDC, PAYER, 1_000_000, &msg_salt(4), "E02_PAUSADO");
    ctx(CAPTURER, 0, NOW);
    c.release(id.clone(), None);
    c.refund(id2.clone());
    ctx(PAYER, 0, RECLAIM);
    c.reclaim(id3.clone());
    ctx(WORKER, 0, NOW);
    let _ = c.withdraw();
    ctx(OTHER, 0, NOW);
    let _ = c.withdraw_for(acc(PAYER));
    ctx(OTHER, 0, RECLAIM + RETENTION_NS);
    c.compact(id2);
    assert_invariante(&c, &TODAS);
    // El guardian no despausa; el owner si.
    ctx(GUARDIAN, 1, NOW);
    expect_panic("ARRAS_E40", || c.unpause());
    ctx(OWNER, 1, NOW);
    c.unpause();
    assert!(!c.get_config().paused);
    authorize(&mut c, PAYER, 1_000_000, &msg_salt(5));
    // El owner tambien puede pausar.
    ctx(OWNER, 1, NOW);
    c.pause();
    assert!(c.get_config().paused);
}

/// U-13 `set_caps_sube`: `ARRAS_E42`. Bajar o dejar igual si.
#[test]
fn u13_set_caps_sube() {
    let mut c = setup();
    ctx(OWNER, 1, NOW);
    expect_panic("ARRAS_E42", || {
        c.set_caps(Some(U128(TOPE_POR_PAGO + 1)), None)
    });
    expect_panic("ARRAS_E42", || {
        c.set_caps(None, Some(U128(TOPE_TVL_GLOBAL + 1)))
    });
    expect_panic("ARRAS_E42", || {
        c.set_caps(Some(U128(1)), Some(U128(TOPE_TVL_GLOBAL + 1)))
    });
    c.set_caps(Some(U128(TOPE_POR_PAGO)), Some(U128(TOPE_TVL_GLOBAL)));
    ctx(OWNER, 1, NOW);
    c.set_caps(Some(U128(5_000_000)), None);
    let cfg = c.get_config();
    assert_eq!(
        (cfg.tope_por_pago, cfg.tope_tvl_global),
        (U128(5_000_000), U128(TOPE_TVL_GLOBAL))
    );
    assert_eq!(
        logs(),
        vec![format!(
            r#"EVENT_JSON:{{"standard":"arras","version":"1.0.0","event":"caps_set","data":[{{"by":"{OWNER}","tope_por_pago":"5000000","tope_tvl_global":"{TOPE_TVL_GLOBAL}"}}]}}"#
        )]
    );
    // El tope nuevo vale para authorize.
    assert_rechazo(
        &mut c,
        USDC,
        PAYER,
        5_000_001,
        &msg_salt(1),
        "E06_TOPE_POR_PAGO",
    );
    expect_panic("ARRAS_E42", || {
        ctx(OWNER, 1, NOW);
        c.set_caps(Some(U128(5_000_001)), None)
    });
}

/// U-21 `admin_no_owner`: cada metodo admin -> `E40`/`E41`; sin yocto -> panic de near-sdk.
#[test]
fn u21_admin_no_owner() {
    let mut c = setup();
    for quien in [OTHER, GUARDIAN, CAPTURER, PAYER, CONTRACT, FEES] {
        ctx(quien, 1, NOW);
        expect_panic("ARRAS_E40", || {
            c.register_operator("kk".into(), operator_input())
        });
        expect_panic("ARRAS_E40", || {
            c.update_operator(OP.into(), OperatorPatch::default())
        });
        expect_panic("ARRAS_E40", || c.set_caps(Some(U128(1)), None));
        expect_panic("ARRAS_E40", || c.unpause());
        if quien != GUARDIAN {
            expect_panic("ARRAS_E41", || c.pause());
        }
    }
    for deposito in [0u128, 2] {
        ctx(OWNER, deposito, NOW);
        expect_panic(YOCTO_MSG, || {
            c.register_operator("kk".into(), operator_input())
        });
        expect_panic(YOCTO_MSG, || {
            c.update_operator(OP.into(), OperatorPatch::default())
        });
        expect_panic(YOCTO_MSG, || c.set_caps(Some(U128(1)), None));
        expect_panic(YOCTO_MSG, || c.pause());
        expect_panic(YOCTO_MSG, || c.unpause());
        ctx(GUARDIAN, deposito, NOW);
        expect_panic(YOCTO_MSG, || c.pause());
    }
    // Nada cambio.
    assert!(!c.get_config().paused);
    assert_eq!(c.get_config().tope_por_pago, U128(TOPE_POR_PAGO));
    assert_eq!(
        c.get_operator(OP.into()).unwrap().capturers,
        vec![acc(CAPTURER)]
    );
    assert_eq!(c.get_operator("kk".into()), None);
}

/// U-23b: authorize con 1300 -> `update_operator(fee_bps=1800, fee_recipient=otro)` -> release: 130000/870000 al
/// `fee_recipient` viejo; un pago nuevo usa la tarifa nueva.
#[test]
fn u23b_fee_congelada_con_update_operator() {
    let (mut c, id) = setup_v1();
    ctx(OWNER, 1, NOW);
    c.update_operator(
        OP.into(),
        OperatorPatch {
            fee_bps: Some(1800),
            fee_recipient: Some(acc(OTHER)),
            ..Default::default()
        },
    );
    assert_eq!(
        logs(),
        vec![format!(
            r#"EVENT_JSON:{{"standard":"arras","version":"1.0.0","event":"operator_updated","data":[{{"operator_id":"em","fee_bps":1800,"fee_recipient":"{OTHER}"}}]}}"#
        )]
    );
    ctx(CAPTURER, 0, NOW);
    c.release(id, None);
    assert_eq!(
        (
            c.claimable_of(&acc(FEES)),
            c.claimable_of(&acc(WORKER)),
            c.claimable_of(&acc(OTHER))
        ),
        (130_000, 870_000, 0)
    );
    // Un pago nuevo firmado con fee_bps_max 1300 ya no pasa (E12); con 1800 si, y su fee va al nuevo.
    assert_rechazo(
        &mut c,
        USDC,
        PAYER,
        1_000_000,
        &msg_salt(2),
        "E12_FEE_MAYOR_AL_MAXIMO",
    );
    let mut m = msg_v1();
    m.fee_bps_max = 1800;
    let id2 = authorize(&mut c, PAYER, 1_000_000, &m.to_canonical_json());
    ctx(CAPTURER, 0, NOW);
    c.release(id2, None);
    assert_eq!(c.claimable_of(&acc(OTHER)), 180_000);
}

/// `update_operator`: E04, E43, E42 (los topes solo bajan) y los capturers nuevos valen para pagos vivos.
#[test]
fn update_operator_guardas_y_capturers() {
    let (mut c, id) = setup_v1();
    ctx(OWNER, 1, NOW);
    expect_panic("ARRAS_E04", || {
        c.update_operator("kk".into(), OperatorPatch::default())
    });
    expect_panic("ARRAS_E43", || {
        c.update_operator(
            OP.into(),
            OperatorPatch {
                fee_bps: Some(1801),
                ..Default::default()
            },
        )
    });
    expect_panic("ARRAS_E42", || {
        c.update_operator(
            OP.into(),
            OperatorPatch {
                tope_tvl_operador: Some(U128(TOPE_TVL_OPERADOR + 1)),
                ..Default::default()
            },
        )
    });
    expect_panic("ARRAS_E42", || {
        c.update_operator(
            OP.into(),
            OperatorPatch {
                tope_tvl_por_payer: Some(U128(TOPE_TVL_POR_PAYER + 1)),
                ..Default::default()
            },
        )
    });
    c.update_operator(
        OP.into(),
        OperatorPatch {
            capturers: Some(vec![acc(OTHER)]),
            min_amount: Some(U128(20_000)),
            tope_tvl_operador: Some(U128(TOPE_TVL_OPERADOR - 1)),
            tope_tvl_por_payer: Some(U128(TOPE_TVL_POR_PAYER)),
            sponsor_max_por_payer: Some(U128(SPONSOR_MAX * 2)),
            ..Default::default()
        },
    );
    let op = c.get_operator(OP.into()).unwrap();
    assert_eq!(op.capturers, vec![acc(OTHER)]);
    assert_eq!(
        (
            op.min_amount,
            op.tope_tvl_operador,
            op.sponsor_max_por_payer
        ),
        (
            U128(20_000),
            U128(TOPE_TVL_OPERADOR - 1),
            U128(SPONSOR_MAX * 2)
        )
    );
    assert_eq!(op.tvl_operador, U128(1_000_000), "update no toca el TVL");
    ctx(CAPTURER, 0, NOW);
    expect_panic("ARRAS_E21", || c.release(id.clone(), None));
    ctx(OTHER, 0, NOW);
    c.release(id, None);
    assert_rechazo(
        &mut c,
        USDC,
        PAYER,
        19_999,
        &msg_salt(2),
        "E05_DEBAJO_DEL_MINIMO",
    );
}
