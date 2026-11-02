//! Disputa: `open_dispute`, `resolve_dispute`, `resolve_expired_dispute`, `dispute_deadline` en el `msg`, el storage
//! de la disputa y `compact` despues de una resolucion.

use super::common::*;
use crate::*;

const RAZON: &str = "1111111111111111111111111111111111111111111111111111111111111111";
const COMPACT_AT: u64 = RECLAIM + RETENTION_NS;

/// V-1 autorizado con `dispute_deadline` = `RELEASE` (antes de `reclaim_after`).
fn con_pago() -> (Arras, PaymentId) {
    let mut c = setup();
    let msg = msg_v2(msg_v1(), Some(RELEASE), None);
    let id = authorize(&mut c, PAYER, USDC_UNIT, &msg);
    (c, id)
}

fn abrir(c: &mut Arras, quien: &str, ts: u64, id: &str) {
    ctx(quien, 0, ts);
    c.open_dispute(id.to_string(), RAZON.into());
}

/// Una llamada que tiene que fallar con `needle` sin cambiar nada.
fn falla(c: &mut Arras, quien: &str, ts: u64, needle: &str, f: impl FnOnce(&mut Arras)) {
    ctx(quien, 0, ts);
    let antes = snapshot(c, PAYER);
    let pagos_antes: Vec<_> = c.payments.values().cloned().collect();
    expect_panic(needle, || f(c));
    assert_eq!(snapshot(c, PAYER), antes, "estado intacto tras {needle}");
    let pagos: Vec<_> = c.payments.values().cloned().collect();
    assert_eq!(pagos, pagos_antes, "pagos intactos tras {needle}");
}

#[test]
fn dispute_deadline_en_el_msg() {
    let mut c = setup();
    // Sin dispute_deadline: vale reclaim_after.
    let id = authorize(&mut c, PAYER, USDC_UNIT, &msg_v1().to_canonical_json());
    assert_eq!(c.get_payment(id).unwrap().dispute_deadline, U64(RECLAIM));
    // En el borde: == reclaim_after pasa; > reclaim_after y <= now son E11.
    let mut m = msg_v1();
    m.salt[15] = 2;
    let id = authorize(
        &mut c,
        PAYER,
        USDC_UNIT,
        &msg_v2(m.clone(), Some(RECLAIM), None),
    );
    assert_eq!(c.get_payment(id).unwrap().dispute_deadline, U64(RECLAIM));
    m.salt[15] = 3;
    for d in [RECLAIM + 1, NOW, NOW - 1, 0] {
        assert_rechazo(
            &mut c,
            USDC,
            PAYER,
            USDC_UNIT,
            &msg_v2(m.clone(), Some(d), None),
            "E11_PLAZOS",
        );
    }
    let id = authorize(&mut c, PAYER, USDC_UNIT, &msg_v2(m, Some(NOW + 1), None));
    let p = c.get_payment(id).unwrap();
    assert_eq!(p.dispute_deadline, U64(NOW + 1));
    assert_eq!(
        p.arbiter_window_ns,
        U64(ARBITER_WINDOW),
        "plazo del arbitro congelado"
    );
    assert_eq!(p.dispute, None);
}

#[test]
fn open_dispute_congela_lo_vivo() {
    let (mut c, id) = con_pago();
    abrir(&mut c, PAYER, NOW + 5, &id);
    let p = c.get_payment(id.clone()).unwrap();
    assert_eq!(p.status, PaymentStatus::Disputed);
    assert_eq!(
        p.dispute,
        Some(Dispute {
            opened_by: Party::Payer,
            reason_hash: RAZON.into(),
            opened_at: U64(NOW + 5),
            resolved_at: None,
        })
    );
    assert_eq!(
        logs(),
        vec![format!(
            r#"EVENT_JSON:{{"standard":"arras","version":"1.0.0","event":"dispute_opened","data":[{{"payment_id":"{id}","opened_by":"{PAYER}","reason_hash":"{RAZON}","amount":"1000000","resolve_by":"{}"}}]}}"#,
            NOW + 5 + ARBITER_WINDOW
        )]
    );
    // Nada lo mueve salvo el arbitro o el vencimiento.
    falla(
        &mut c,
        CAPTURER,
        NOW + 6,
        "ARRAS_E22: estado invalido para release",
        |c| c.release(id.clone(), None),
    );
    falla(
        &mut c,
        CAPTURER,
        NOW + 6,
        "ARRAS_E22: estado invalido para refund",
        |c| c.refund(id.clone()),
    );
    falla(
        &mut c,
        WORKER,
        NOW + 6,
        "ARRAS_E22: estado invalido para refund",
        |c| c.refund(id.clone()),
    );
    falla(
        &mut c,
        PAYER,
        RECLAIM,
        "ARRAS_E22: estado invalido para reclaim",
        |c| c.reclaim(id.clone()),
    );
    falla(
        &mut c,
        WORKER,
        NOW + 6,
        "ARRAS_E22: estado invalido para open_dispute",
        |c| c.open_dispute(id.clone(), RAZON.into()),
    );
    falla(
        &mut c,
        OTHER,
        RECLAIM + 10 * DAY_NS,
        "ARRAS_E22: estado invalido para compact",
        |c| c.compact(id.clone()),
    );
    // Lo congelado sigue contando en los TVL.
    assert_eq!(c.payer_tvl_of(OP, &acc(PAYER)), USDC_UNIT);
    assert_invariante(&c, &TODAS);
}

#[test]
fn open_dispute_guardas_en_orden() {
    let (mut c, id) = con_pago();
    // E20.
    falla(&mut c, PAYER, NOW, "ARRAS_E20", |c| {
        c.open_dispute(
            "Sc4D4Jn3ErWE68FckdDTLzThk1FTR1ZfLzuBUE4EeRm".into(),
            RAZON.into(),
        )
    });
    // E60: ni el capturer, ni el arbitro, ni un tercero (y antes que E61 y E66).
    for quien in [CAPTURER, ARBITER, OTHER, OWNER] {
        falla(&mut c, quien, RECLAIM, "ARRAS_E60", |c| {
            c.open_dispute(id.clone(), "x".into())
        });
    }
    // E61 en el borde: now == dispute_deadline ya no; now == dispute_deadline - 1 si.
    falla(&mut c, PAYER, RELEASE, "ARRAS_E61", |c| {
        c.open_dispute(id.clone(), "x".into())
    });
    // E66: reason_hash mal formado (el plazo esta bien).
    for malo in [
        String::new(),
        "A".repeat(64),
        "0".repeat(63),
        "0".repeat(65),
        format!("0x{}", "0".repeat(62)),
    ] {
        falla(&mut c, PAYER, RELEASE - 1, "ARRAS_E66", |c| {
            c.open_dispute(id.clone(), malo.clone())
        });
    }
    abrir(&mut c, WORKER, RELEASE - 1, &id);
    assert_eq!(
        c.get_payment(id.clone())
            .unwrap()
            .dispute
            .unwrap()
            .opened_by,
        Party::Receiver
    );
    // E22 antes que E61: un pago liberado, fuera de plazo.
    let mut m = msg_v1();
    m.salt[15] = 9;
    let id2 = authorize(&mut c, PAYER, USDC_UNIT, &m.to_canonical_json());
    ctx(CAPTURER, 0, NOW);
    c.release(id2.clone(), None);
    falla(&mut c, PAYER, RECLAIM, "ARRAS_E22", |c| {
        c.open_dispute(id2.clone(), RAZON.into())
    });
    assert_invariante(&c, &TODAS);
}

#[test]
fn resolve_dispute_reparte_con_la_tarifa_sobre_lo_del_receiver() {
    let (mut c, id) = con_pago();
    abrir(&mut c, PAYER, NOW, &id);
    ctx(ARBITER, 0, NOW + 10);
    c.resolve_dispute(id.clone(), U128(400_000), U128(600_000));
    let p = c.get_payment(id.clone()).unwrap();
    assert_eq!(p.status, PaymentStatus::Resolved);
    assert_eq!((p.released.0, p.refunded.0), (400_000, 600_000));
    assert_eq!(p.dispute.unwrap().resolved_at, Some(U64(NOW + 10)));
    assert_eq!(c.claimable_of(&acc(WORKER)), 348_000);
    assert_eq!(c.claimable_of(&acc(FEES)), 52_000);
    assert_eq!(c.claimable_of(&acc(PAYER)), 600_000);
    assert_eq!(c.payer_tvl_of(OP, &acc(PAYER)), 0);
    assert_eq!(c.get_operator(OP.into()).unwrap().tvl_operador.0, 0);
    assert_eq!(c.tvl_global, USDC_UNIT, "baja recien con los retiros");
    assert_eq!(
        logs(),
        vec![format!(
            r#"EVENT_JSON:{{"standard":"arras","version":"1.0.0","event":"dispute_resolved","data":[{{"payment_id":"{id}","by":"{ARBITER}","to_receiver":"400000","net":"348000","fee":"52000","to_payer":"600000","expired":false}}]}}"#
        )]
    );
    // Terminal: nada mas.
    falla(
        &mut c,
        ARBITER,
        NOW + 11,
        "ARRAS_E22: estado invalido para resolve_dispute",
        |c| c.resolve_dispute(id.clone(), U128(0), U128(0)),
    );
    falla(&mut c, OTHER, RECLAIM + ARBITER_WINDOW, "ARRAS_E22", |c| {
        c.resolve_expired_dispute(id.clone())
    });
    assert_invariante(&c, &TODAS);
}

#[test]
fn resolve_dispute_bordes_del_reparto() {
    for (to_r, to_p) in [(USDC_UNIT, 0), (0, USDC_UNIT), (1, USDC_UNIT - 1)] {
        let (mut c, id) = con_pago();
        abrir(&mut c, WORKER, NOW, &id);
        ctx(ARBITER, 0, NOW);
        c.resolve_dispute(id.clone(), U128(to_r), U128(to_p));
        let fee = to_r * 1300 / 10_000;
        assert_eq!(c.claimable_of(&acc(WORKER)), to_r - fee);
        assert_eq!(c.claimable_of(&acc(FEES)), fee);
        assert_eq!(c.claimable_of(&acc(PAYER)), to_p);
        assert_invariante(&c, &TODAS);
    }
}

#[test]
fn resolve_dispute_guardas_en_orden() {
    let (mut c, id) = con_pago();
    // E20.
    falla(&mut c, ARBITER, NOW, "ARRAS_E20", |c| {
        c.resolve_dispute(
            "Sc4D4Jn3ErWE68FckdDTLzThk1FTR1ZfLzuBUE4EeRm".into(),
            U128(0),
            U128(0),
        )
    });
    // E62 antes que E22: el pago no esta en disputa y quien llama no es el arbitro.
    for quien in [CAPTURER, PAYER, WORKER, OWNER, OTHER] {
        falla(&mut c, quien, NOW, "ARRAS_E62", |c| {
            c.resolve_dispute(id.clone(), U128(USDC_UNIT), U128(0))
        });
    }
    // E22: el arbitro sobre un pago sin disputa.
    falla(
        &mut c,
        ARBITER,
        NOW,
        "ARRAS_E22: estado invalido para resolve_dispute",
        |c| c.resolve_dispute(id.clone(), U128(USDC_UNIT), U128(0)),
    );
    abrir(&mut c, PAYER, NOW, &id);
    let resolve_by = NOW + ARBITER_WINDOW;
    // E65 antes que E63: vencido y con un reparto malo.
    falla(&mut c, ARBITER, resolve_by, "ARRAS_E65", |c| {
        c.resolve_dispute(id.clone(), U128(1), U128(1))
    });
    // E63: la suma no es lo vivo (de menos, de mas y desbordando).
    for (r, p) in [
        (USDC_UNIT - 1, 0),
        (USDC_UNIT, 1),
        (u128::MAX, 1),
        (u128::MAX, u128::MAX),
        (0, 0),
    ] {
        falla(&mut c, ARBITER, resolve_by - 1, "ARRAS_E63", |c| {
            c.resolve_dispute(id.clone(), U128(r), U128(p))
        });
    }
    // En el ultimo nanosegundo del plazo, el arbitro todavia resuelve.
    ctx(ARBITER, 0, resolve_by - 1);
    c.resolve_dispute(id, U128(USDC_UNIT), U128(0));
    assert_invariante(&c, &TODAS);
}

#[test]
fn arbitro_rotado_y_plazo_congelado() {
    let (mut c, id) = con_pago();
    abrir(&mut c, PAYER, NOW, &id);
    ctx(OWNER, 1, NOW);
    c.update_operator(
        OP.into(),
        OperatorPatch {
            arbiter: Some(acc("arbiter-nuevo.testnet")),
            arbiter_window_ns: Some(U64(MIN_ARBITER_WINDOW_NS)),
            ..Default::default()
        },
    );
    // El plazo de este pago sigue siendo el congelado (3 dias), no el minuto nuevo.
    falla(
        &mut c,
        OTHER,
        NOW + MIN_ARBITER_WINDOW_NS,
        "ARRAS_E64",
        |c| c.resolve_expired_dispute(id.clone()),
    );
    // El arbitro viejo ya no resuelve; el nuevo si (rotar una llave comprometida).
    falla(&mut c, ARBITER, NOW + 1, "ARRAS_E62", |c| {
        c.resolve_dispute(id.clone(), U128(USDC_UNIT), U128(0))
    });
    ctx("arbiter-nuevo.testnet", 0, NOW + 1);
    c.resolve_dispute(id, U128(0), U128(USDC_UNIT));
    assert_eq!(c.claimable_of(&acc(PAYER)), USDC_UNIT);
    // Un pago nuevo toma el plazo nuevo.
    let mut m = msg_v1();
    m.salt[15] = 7;
    let id2 = authorize(&mut c, PAYER, USDC_UNIT, &m.to_canonical_json());
    assert_eq!(
        c.get_payment(id2).unwrap().arbiter_window_ns,
        U64(MIN_ARBITER_WINDOW_NS)
    );
    assert_invariante(&c, &TODAS);
}

/// Vencido el plazo, lo vivo va a quien no abrio la disputa: si la abrio el payer, al receiver (con la tarifa).
#[test]
fn vencida_la_abrio_el_payer_cobra_el_receiver() {
    let mut c = setup();
    let id = authorize(&mut c, PAYER, USDC_UNIT, &msg_v1().to_canonical_json());
    // Liberacion parcial y despues la disputa sobre el resto.
    ctx(CAPTURER, 0, NOW);
    c.release(id.clone(), Some(U128(400_000)));
    abrir(&mut c, PAYER, NOW + 1, &id);
    let resolve_by = NOW + 1 + ARBITER_WINDOW;
    falla(&mut c, OTHER, resolve_by - 1, "ARRAS_E64", |c| {
        c.resolve_expired_dispute(id.clone())
    });
    // Cualquiera la cierra; el arbitro ya no puede.
    falla(&mut c, ARBITER, resolve_by, "ARRAS_E65", |c| {
        c.resolve_dispute(id.clone(), U128(0), U128(600_000))
    });
    ctx(OTHER, 0, resolve_by);
    c.resolve_expired_dispute(id.clone());
    assert_eq!(
        logs(),
        vec![format!(
            r#"EVENT_JSON:{{"standard":"arras","version":"1.0.0","event":"dispute_resolved","data":[{{"payment_id":"{id}","by":"{OTHER}","to_receiver":"600000","net":"522000","fee":"78000","to_payer":"0","expired":true}}]}}"#
        )]
    );
    let p = c.get_payment(id).unwrap();
    assert_eq!(
        (p.released.0, p.refunded.0, p.status),
        (USDC_UNIT, 0, PaymentStatus::Resolved)
    );
    assert_eq!(c.claimable_of(&acc(WORKER)), 348_000 + 522_000);
    assert_eq!(c.claimable_of(&acc(FEES)), 52_000 + 78_000);
    assert_eq!(c.claimable_of(&acc(PAYER)), 0);
    assert_invariante(&c, &TODAS);
}

/// Si la abrio el receiver, lo vivo vuelve al payer; lo liberado antes queda liberado.
#[test]
fn vencida_la_abrio_el_receiver_vuelve_al_payer() {
    let mut c = setup();
    let id = authorize(&mut c, PAYER, USDC_UNIT, &msg_v1().to_canonical_json());
    ctx(CAPTURER, 0, NOW);
    c.release(id.clone(), Some(U128(400_000)));
    abrir(&mut c, WORKER, RELEASE + 1, &id);
    ctx(OTHER, 0, RELEASE + 1 + ARBITER_WINDOW);
    c.resolve_expired_dispute(id.clone());
    let p = c.get_payment(id).unwrap();
    assert_eq!((p.released.0, p.refunded.0), (400_000, 600_000));
    assert_eq!(
        c.claimable_of(&acc(WORKER)),
        348_000,
        "lo liberado queda liberado"
    );
    assert_eq!(c.claimable_of(&acc(PAYER)), 600_000);
    assert_invariante(&c, &TODAS);
}

#[test]
fn disputas_funcionan_en_pausa() {
    let (mut c, id) = con_pago();
    c.paused = true;
    abrir(&mut c, PAYER, NOW, &id);
    ctx(ARBITER, 0, NOW);
    c.resolve_dispute(id, U128(500_000), U128(500_000));
    let mut m = msg_v1();
    m.salt[15] = 3;
    c.paused = false;
    let id2 = authorize(&mut c, PAYER, USDC_UNIT, &m.to_canonical_json());
    c.paused = true;
    abrir(&mut c, WORKER, NOW, &id2);
    ctx(OTHER, 0, NOW + ARBITER_WINDOW);
    c.resolve_expired_dispute(id2);
    assert_invariante(&c, &TODAS);
}

/// El storage de la disputa ya estaba cobrado: abrir y resolver crecen el `Payment` en exactamente
/// `DISPUTE_RESERVE_BYTES`, y `compact` devuelve lo mismo con o sin disputa.
#[test]
fn storage_de_la_disputa_esta_prepagado() {
    let (mut c, id) = con_pago();
    // Un segundo pago con un id del mismo largo (asi sus registros pesan lo mismo).
    let mut m = msg_v1();
    let mut n = 1u8;
    loop {
        m.salt[15] = n;
        if id_of(PAYER, USDC_UNIT, &m.to_canonical_json()).len() == id.len() {
            break;
        }
        n += 1;
    }
    let id2 = authorize(&mut c, PAYER, USDC_UNIT, &m.to_canonical_json());
    let charged = c.get_payment(id.clone()).unwrap().storage_charged.0;
    // Abrir + resolver: el `Payment` crece 86 bytes, sin cobrar nada a nadie. (`env::storage_usage()` se reinicia con
    // cada contexto y aca hay dos llamadores: se mide el borsh del valor.)
    let largo = |c: &Arras| {
        near_sdk::borsh::to_vec(&c.get_payment(id.clone()).unwrap())
            .unwrap()
            .len() as u64
    };
    let antes = snapshot(&mut c, PAYER);
    let largo_antes = largo(&c);
    abrir(&mut c, WORKER, NOW, &id);
    ctx(ARBITER, 0, NOW + 1);
    c.resolve_dispute(id.clone(), U128(USDC_UNIT), U128(0));
    let despues = snapshot(&mut c, PAYER);
    assert_eq!(largo(&c) - largo_antes, DISPUTE_RESERVE_BYTES);
    assert_eq!(despues.storage_payer, antes.storage_payer);
    // El otro pago termina sin disputa.
    ctx(CAPTURER, 0, NOW);
    c.refund(id2.clone());
    // Compactar los dos devuelve lo mismo.
    let mut devuelto = vec![];
    for i in [id, id2] {
        ctx(OTHER, 0, COMPACT_AT);
        let a = c.storage.get(&acc(PAYER)).unwrap().available;
        c.compact(i);
        devuelto.push(c.storage.get(&acc(PAYER)).unwrap().available - a);
    }
    assert_eq!(devuelto[0], devuelto[1]);
    assert!(devuelto[0] > 0 && devuelto[0] < charged);
    assert_invariante(&c, &TODAS);
}

#[test]
fn dispute_reserve_bytes_es_el_borsh_real() {
    let none: Option<Dispute> = None;
    let max = Some(Dispute {
        opened_by: Party::Receiver,
        reason_hash: RAZON.into(),
        opened_at: U64(u64::MAX),
        resolved_at: Some(U64(u64::MAX)),
    });
    let n = near_sdk::borsh::to_vec(&none).unwrap().len();
    let m = near_sdk::borsh::to_vec(&max).unwrap().len();
    assert_eq!((m - n) as u64, DISPUTE_RESERVE_BYTES);
}

/// La retencion de `compact` cuenta desde la resolucion si fue despues de `reclaim_after`.
#[test]
fn compact_espera_7_dias_desde_la_resolucion() {
    let (mut c, id) = con_pago();
    abrir(&mut c, PAYER, RELEASE - 1, &id);
    let resuelto = RELEASE - 1 + ARBITER_WINDOW;
    assert!(resuelto > RECLAIM);
    ctx(OTHER, 0, resuelto);
    c.resolve_expired_dispute(id.clone());
    falla(&mut c, OTHER, COMPACT_AT, "ARRAS_E28", |c| {
        c.compact(id.clone())
    });
    falla(
        &mut c,
        OTHER,
        resuelto + RETENTION_NS - 1,
        "ARRAS_E28",
        |c| c.compact(id.clone()),
    );
    ctx(OTHER, 0, resuelto + RETENTION_NS);
    c.compact(id.clone());
    assert_eq!(c.get_payment(id), None);
    // Resuelto antes de reclaim_after: cuenta reclaim_after.
    let mut m = msg_v1();
    m.salt[15] = 4;
    let id2 = authorize(&mut c, PAYER, USDC_UNIT, &m.to_canonical_json());
    abrir(&mut c, PAYER, NOW, &id2);
    ctx(ARBITER, 0, NOW);
    c.resolve_dispute(id2.clone(), U128(0), U128(USDC_UNIT));
    falla(&mut c, OTHER, COMPACT_AT - 1, "ARRAS_E28", |c| {
        c.compact(id2.clone())
    });
    ctx(OTHER, 0, COMPACT_AT);
    c.compact(id2);
    assert_invariante(&c, &TODAS);
}

#[test]
fn disputa_de_un_pago_del_presupuesto() {
    let mut c = setup();
    deposit(
        &mut c,
        PAYER,
        AGENT,
        3 * USDC_UNIT,
        (2 * USDC_UNIT, 3 * USDC_UNIT),
    );
    ctx(AGENT, 0, NOW);
    let mut a = afb(2 * USDC_UNIT, 1);
    a.dispute_deadline = Some(RELEASE);
    let id = call_afb(&mut c, PAYER, &a);
    // El agente no es parte: no abre disputas (el owner si, como payer).
    falla(&mut c, AGENT, NOW, "ARRAS_E60", |c| {
        c.open_dispute(id.clone(), RAZON.into())
    });
    abrir(&mut c, PAYER, NOW, &id);
    ctx(ARBITER, 0, NOW);
    c.resolve_dispute(id, U128(0), U128(2 * USDC_UNIT));
    assert_eq!(
        c.claimable_of(&acc(PAYER)),
        2 * USDC_UNIT,
        "vuelve al owner"
    );
    assert_eq!(c.budget_of(PAYER, AGENT, OP).unwrap().saldo.0, USDC_UNIT);
    assert_invariante(&c, &TODAS);
}
