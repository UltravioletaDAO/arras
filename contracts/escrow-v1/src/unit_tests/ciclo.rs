//! C5: `release`, `refund` y `reclaim`. U-09, U-10, U-12, U-20, U-22, U-23 (U-11, la pausa, en C7).

use super::common::*;
use crate::*;

const BEFORE_RELEASE: u64 = RELEASE - 1;

/// U-09 `release_parcial_y_total`: 400000 y luego el resto: `claimable[worker] = 870000`,
/// `claimable[fee] = 130000`, `Released`.
#[test]
fn u09_release_parcial_y_total() {
    let (mut c, id) = setup_v1();
    ctx(CAPTURER, 0, NOW);
    c.release(id.clone(), Some(U128(400_000)));
    let p = c.get_payment(id.clone()).unwrap();
    assert_eq!(
        (p.status, p.released),
        (PaymentStatus::PartiallyReleased, U128(400_000))
    );
    assert_eq!(
        (c.claimable_of(&acc(WORKER)), c.claimable_of(&acc(FEES))),
        (348_000, 52_000)
    );
    assert_eq!(
        logs(),
        vec![format!(
            r#"EVENT_JSON:{{"standard":"arras","version":"1.0.0","event":"released","data":[{{"payment_id":"{id}","amount":"400000","net":"348000","fee":"52000"}}]}}"#
        )]
    );
    assert_invariante(&c, &TODAS);
    ctx(CAPTURER, 0, BEFORE_RELEASE);
    c.release(id.clone(), None);
    let p = c.get_payment(id.clone()).unwrap();
    assert_eq!(
        (p.status, p.released, p.refunded),
        (PaymentStatus::Released, U128(1_000_000), U128(0))
    );
    assert_eq!(c.claimable_of(&acc(WORKER)), 870_000);
    assert_eq!(c.claimable_of(&acc(FEES)), 130_000);
    assert_eq!(c.claimable_of(&acc(PAYER)), 0);
    // release no baja tvl_global (U-19); si tvl_operador y payer_tvl.
    assert_eq!(c.tvl_global, 1_000_000);
    assert_eq!(c.get_operator(OP.into()).unwrap().tvl_operador, U128(0));
    assert_eq!(c.payer_tvl_of(OP, &acc(PAYER)), 0);
    assert_invariante(&c, &TODAS);
    expect_panic("ARRAS_E22: estado invalido para release", || {
        c.release(id.clone(), None)
    });
}

/// U-10 `release_tarde`: `ARRAS_E23` desde `release_deadline` (inclusive).
#[test]
fn u10_release_tarde() {
    let (mut c, id) = setup_v1();
    ctx(CAPTURER, 0, RELEASE);
    expect_panic("ARRAS_E23", || c.release(id.clone(), None));
    ctx(CAPTURER, 0, RECLAIM + 1);
    expect_panic("ARRAS_E23", || c.release(id.clone(), Some(U128(1))));
    ctx(CAPTURER, 0, BEFORE_RELEASE);
    c.release(id.clone(), Some(U128(1)));
}

/// U-12 `reclaim_antes`: `ARRAS_E27` antes de `reclaim_after`; desde `reclaim_after` el payer recupera lo vivo.
#[test]
fn u12_reclaim_antes() {
    let (mut c, id) = setup_v1();
    for t in [NOW, RELEASE, RECLAIM - 1] {
        ctx(PAYER, 0, t);
        expect_panic("ARRAS_E27", || c.reclaim(id.clone()));
    }
    ctx(CAPTURER, 0, NOW);
    c.release(id.clone(), Some(U128(250_000)));
    ctx(PAYER, 0, RECLAIM);
    c.reclaim(id.clone());
    let p = c.get_payment(id.clone()).unwrap();
    assert_eq!(
        (p.status, p.released, p.refunded),
        (PaymentStatus::Reclaimed, U128(250_000), U128(750_000))
    );
    assert_eq!(c.claimable_of(&acc(PAYER)), 750_000);
    assert_eq!(
        logs(),
        vec![format!(
            r#"EVENT_JSON:{{"standard":"arras","version":"1.0.0","event":"reclaimed","data":[{{"payment_id":"{id}","amount":"750000"}}]}}"#
        )]
    );
    assert_invariante(&c, &TODAS);
    expect_panic("ARRAS_E22: estado invalido para reclaim", || {
        c.reclaim(id.clone())
    });
}

/// U-20 `release_no_capturer`: receiver y tercero -> `ARRAS_E21` (y el owner o el payer tampoco).
#[test]
fn u20_release_no_capturer() {
    let (mut c, id) = setup_v1();
    for quien in [WORKER, OTHER, OWNER, PAYER, GUARDIAN, FEES, CONTRACT] {
        ctx(quien, 0, NOW);
        expect_panic("ARRAS_E21", || c.release(id.clone(), None));
        expect_panic("ARRAS_E21", || c.release(id.clone(), Some(U128(1))));
    }
    assert_eq!(c.get_payment(id).unwrap().released, U128(0));
}

/// U-22 `refund_reclaim_terceros`: `E25` y `E26`.
#[test]
fn u22_refund_reclaim_terceros() {
    let (mut c, id) = setup_v1();
    for quien in [OTHER, PAYER, OWNER, FEES] {
        ctx(quien, 0, NOW);
        expect_panic("ARRAS_E25", || c.refund(id.clone()));
    }
    for quien in [OTHER, WORKER, CAPTURER, OWNER] {
        ctx(quien, 0, RECLAIM);
        expect_panic("ARRAS_E26", || c.reclaim(id.clone()));
    }
    // El receiver renuncia: refund OK.
    ctx(WORKER, 0, NOW);
    c.refund(id.clone());
    assert_eq!(
        c.get_payment(id.clone()).unwrap().status,
        PaymentStatus::Refunded
    );
    assert_eq!(c.claimable_of(&acc(PAYER)), 1_000_000);
    // Y el capturer tambien puede reembolsar (otro pago).
    let id2 = authorize(&mut c, PAYER, 1_000_000, &msg_salt(2));
    ctx(CAPTURER, 0, RECLAIM + 1);
    c.refund(id2.clone());
    assert_eq!(c.claimable_of(&acc(PAYER)), 2_000_000);
    assert_eq!(
        logs(),
        vec![format!(
            r#"EVENT_JSON:{{"standard":"arras","version":"1.0.0","event":"refunded","data":[{{"payment_id":"{id2}","amount":"1000000"}}]}}"#
        )]
    );
    assert_invariante(&c, &TODAS);
}

/// U-23 `fee_congelada`: authorize con 1300 -> el operador pasa a 1800 y otro `fee_recipient` -> release:
/// 130000/870000 al `fee_recipient` viejo. (Con `update_operator` real, en C7: U-23b.)
#[test]
fn u23_fee_congelada() {
    let (mut c, id) = setup_v1();
    {
        let op = c.operators.get_mut(OP).unwrap();
        op.fee_bps = 1800;
        op.fee_recipient = acc(OTHER);
    }
    ctx(CAPTURER, 0, NOW);
    c.release(id.clone(), None);
    assert_eq!(c.claimable_of(&acc(FEES)), 130_000);
    assert_eq!(c.claimable_of(&acc(WORKER)), 870_000);
    assert_eq!(c.claimable_of(&acc(OTHER)), 0);
    let p = c.get_payment(id).unwrap();
    assert_eq!((p.fee_bps, p.fee_recipient), (1300, acc(FEES)));
}

/// Maquina de estados (sec. 7): toda transicion fuera de la tabla es `ARRAS_E22`.
#[test]
fn maquina_de_estados() {
    // Refund tras release parcial: lo liberado queda en claimable; el resto al payer.
    let (mut c, id) = setup_v1();
    ctx(CAPTURER, 0, NOW);
    c.release(id.clone(), Some(U128(400_000)));
    c.refund(id.clone());
    let p = c.get_payment(id.clone()).unwrap();
    assert_eq!(
        (p.status, p.released, p.refunded),
        (PaymentStatus::Refunded, U128(400_000), U128(600_000))
    );
    assert_eq!(
        (
            c.claimable_of(&acc(WORKER)),
            c.claimable_of(&acc(FEES)),
            c.claimable_of(&acc(PAYER))
        ),
        (348_000, 52_000, 600_000)
    );
    assert_invariante(&c, &TODAS);
    // Desde un terminal, nada.
    expect_panic("ARRAS_E22: estado invalido para release", || {
        c.release(id.clone(), Some(U128(1)))
    });
    expect_panic("ARRAS_E22: estado invalido para refund", || {
        c.refund(id.clone())
    });
    ctx(PAYER, 0, RECLAIM);
    expect_panic("ARRAS_E22: estado invalido para reclaim", || {
        c.reclaim(id.clone())
    });
    // Released -> refund y reclaim tambien son E22.
    let id2 = authorize(&mut c, PAYER, 1_000_000, &msg_salt(2));
    ctx(CAPTURER, 0, NOW);
    c.release(id2.clone(), None);
    expect_panic("ARRAS_E22: estado invalido para refund", || {
        c.refund(id2.clone())
    });
    ctx(PAYER, 0, RECLAIM);
    expect_panic("ARRAS_E22: estado invalido para reclaim", || {
        c.reclaim(id2.clone())
    });
}

/// `E24` y `E20`.
#[test]
fn monto_y_pago_inexistente() {
    let (mut c, id) = setup_v1();
    ctx(CAPTURER, 0, NOW);
    expect_panic("ARRAS_E24", || c.release(id.clone(), Some(U128(0))));
    expect_panic("ARRAS_E24", || c.release(id.clone(), Some(U128(1_000_001))));
    c.release(id.clone(), Some(U128(999_999)));
    expect_panic("ARRAS_E24", || c.release(id.clone(), Some(U128(2))));
    c.release(id.clone(), Some(U128(1)));
    assert_eq!(c.get_payment(id).unwrap().status, PaymentStatus::Released);
    let falso = "Sc4D4Jn3ErWE68FckdDTLzThk1FTR1ZfLzuBUE4EeRm".to_string();
    expect_panic("ARRAS_E20", || c.release(falso.clone(), None));
    expect_panic("ARRAS_E20", || c.refund(falso.clone()));
    ctx(PAYER, 0, RECLAIM);
    expect_panic("ARRAS_E20", || c.reclaim(falso));
}

/// La primera guarda que falla decide, en `release`, `refund` y `reclaim`.
#[test]
fn orden_de_las_guardas_del_ciclo() {
    let (mut c, id) = setup_v1();
    let id2 = authorize(&mut c, PAYER, 1_000_000, &msg_salt(2));
    ctx(CAPTURER, 0, NOW);
    c.release(id2.clone(), None); // id2 terminal
    let falso = "Sc4D4Jn3ErWE68FckdDTLzThk1FTR1ZfLzuBUE4EeRm".to_string();
    // release: E20 antes que E21; E21 antes que E22; E22 antes que E23; E23 antes que E24.
    ctx(OTHER, 0, RECLAIM);
    expect_panic("ARRAS_E20", || c.release(falso.clone(), Some(U128(0))));
    expect_panic("ARRAS_E21", || c.release(id2.clone(), Some(U128(0))));
    ctx(CAPTURER, 0, RECLAIM);
    expect_panic("ARRAS_E22", || c.release(id2.clone(), Some(U128(0))));
    expect_panic("ARRAS_E23", || c.release(id.clone(), Some(U128(0))));
    // refund: E20 -> E25 -> E22.
    ctx(OTHER, 0, NOW);
    expect_panic("ARRAS_E20", || c.refund(falso.clone()));
    expect_panic("ARRAS_E25", || c.refund(id2.clone()));
    // reclaim: E20 -> E26 -> E27 -> E22.
    ctx(OTHER, 0, NOW);
    expect_panic("ARRAS_E20", || c.reclaim(falso.clone()));
    expect_panic("ARRAS_E26", || c.reclaim(id2.clone()));
    ctx(PAYER, 0, NOW);
    expect_panic("ARRAS_E27", || c.reclaim(id2.clone()));
    ctx(PAYER, 0, RECLAIM);
    expect_panic("ARRAS_E22", || c.reclaim(id2.clone()));
}
