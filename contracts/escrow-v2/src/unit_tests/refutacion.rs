//! Los hallazgos de la auto-refutacion de v2 (ronda 1, opus/xhigh), cada uno con el test que lo mata. Los R-n son los
//! tests de reproduccion del refutador, invertidos: ahora afirman el rechazo con el estado intacto.

use super::common::*;
use crate::*;

/// Rechazo de `authorize_from_budget` con el estado intacto.
fn afb_falla(c: &mut Arras, agent: &str, ts: u64, owner: &str, a: &Afb, needle: &str) {
    ctx(agent, 0, ts);
    let antes = snapshot(c, owner);
    expect_panic(needle, || call_afb(c, owner, a));
    assert_eq!(snapshot(c, owner), antes, "estado intacto tras {needle}");
}

/// P1-1 (R-1): el agente no puede trabar la plata del owner con plazos lejanos: `reclaim_after <= now + 60 dias`
/// (`E57`), con el borde exacto, y el owner recupera todo con `reclaim` dentro de ese horizonte.
#[test]
fn p1_1_horizonte_de_los_pagos_del_presupuesto() {
    let mut c = setup();
    deposit(
        &mut c,
        PAYER,
        AGENT,
        5 * USDC_UNIT,
        (2 * USDC_UNIT, 3 * USDC_UNIT),
    );
    let limite = NOW + MAX_PLAZO_PRESUPUESTO_NS;
    for reclaim in [u64::MAX, limite + 1] {
        let mut a = afb(2 * USDC_UNIT, 1);
        a.receiver = OTHER.into();
        a.release_deadline = reclaim - 1;
        a.reclaim_after = reclaim;
        a.dispute_deadline = Some(reclaim - 1);
        afb_falla(&mut c, AGENT, NOW, PAYER, &a, "ARRAS_E57");
    }
    // En el borde exacto pasa (con la disputa mas corta que el agente puede elegir, `dispute_deadline =
    // release_deadline`, E67), y el owner reclama en `reclaim_after` sin disputar.
    let mut a = afb(2 * USDC_UNIT, 1);
    a.receiver = OTHER.into();
    a.release_deadline = limite - 1;
    a.reclaim_after = limite;
    a.dispute_deadline = Some(limite - 1);
    ctx(AGENT, 0, NOW);
    let id = call_afb(&mut c, PAYER, &a);
    ctx(PAYER, 0, limite);
    c.reclaim(id.clone());
    assert_eq!(c.claimable_of(&acc(PAYER)), 2 * USDC_UNIT);
    // Y se compacta a los 7 dias: el storage del owner vuelve.
    ctx(OTHER, 0, limite + RETENTION_NS);
    let antes = c.storage.get(&acc(PAYER)).unwrap().available;
    c.compact(id);
    assert!(c.storage.get(&acc(PAYER)).unwrap().available > antes);
    assert_invariante(&c, &[PAYER, WORKER, FEES, OTHER]);
}

/// Orden: E50 (saldo) -> E57 -> E11.
#[test]
fn p1_1_orden_de_e57() {
    let mut c = setup();
    deposit(
        &mut c,
        PAYER,
        AGENT,
        USDC_UNIT,
        (2 * USDC_UNIT, 3 * USDC_UNIT),
    );
    let mut a = afb(2 * USDC_UNIT, 1);
    a.reclaim_after = u64::MAX;
    afb_falla(&mut c, AGENT, NOW, PAYER, &a, "ARRAS_E50");
    let mut a = afb(USDC_UNIT, 1);
    a.reclaim_after = u64::MAX;
    a.release_deadline = NOW;
    afb_falla(&mut c, AGENT, NOW, PAYER, &a, "ARRAS_E57");
    // El ft_transfer_call del propio payer no tiene horizonte: elige sus plazos (como v1).
    let mut m = msg_v1();
    m.release_deadline = u64::MAX - 1;
    m.reclaim_after = u64::MAX;
    authorize(&mut c, PAYER, USDC_UNIT, &m.to_canonical_json());
}

/// P3-1 (R-2): ninguna llave de orden chico pasa como `ticket_pubkey`: los 8 puntos de torsion, con y sin el bit de
/// signo, y las codificaciones no canonicas de la lista de libsodium (`E48`), en `register_operator` y en
/// `update_operator`. La llave de los tests sigue valiendo.
#[test]
fn p3_1_llaves_de_orden_chico() {
    use curve25519_dalek::constants::EIGHT_TORSION;
    let mut chicas: Vec<[u8; 32]> = vec![];
    for p in EIGHT_TORSION {
        let b = p.compress().to_bytes();
        let mut con_signo = b;
        con_signo[31] |= 0x80;
        let mut sin_signo = b;
        sin_signo[31] &= 0x7f;
        chicas.extend([b, con_signo, sin_signo]);
    }
    for k in [0xecu8, 0xed, 0xee] {
        let mut b = [0xffu8; 32];
        b[0] = k;
        b[31] = 0x7f;
        chicas.push(b);
        b[31] = 0xff;
        chicas.push(b);
    }
    let mut c = setup();
    for k in &chicas {
        assert!(crate::admin::es_de_orden_chico(k), "{k:02x?}");
        let pk = PublicKey::from_parts(near_sdk::CurveType::ED25519, k.to_vec()).unwrap();
        ctx(OWNER, 1, NOW);
        let mut i = operator_input();
        i.ticket_pubkey = Some(pk.clone());
        expect_panic("ARRAS_E48", || c.register_operator("kk".into(), i));
        expect_panic("ARRAS_E48", || {
            c.update_operator(
                OP.into(),
                OperatorPatch {
                    ticket_pubkey: Some(pk),
                    ..Default::default()
                },
            )
        });
    }
    assert_eq!(c.get_operator(OP.into()).unwrap().ticket_pubkey, None);
    assert!(!crate::admin::es_de_orden_chico(
        &ticket_signing_key().verifying_key().to_bytes()
    ));
    con_ticket(&mut c);
    assert_eq!(
        c.get_operator(OP.into()).unwrap().ticket_pubkey,
        Some(ticket_pubkey())
    );
    // Con la llave buena, la firma `R = identidad, S = 0` del refutador no vale.
    let mut sig = [0u8; 64];
    sig[0] = 1;
    let falso = TicketArg {
        expiry: U64(RELEASE),
        sig: near_sdk::bs58::encode(sig).into_string(),
    };
    let mut m = msg_v1();
    m.salt[15] = 77;
    assert_rechazo(
        &mut c,
        USDC,
        PAYER,
        USDC_UNIT,
        &msg_v2(m, None, Some(&falso)),
        "E53_TICKET_INVALIDO",
    );
}

/// P3-2 (R-3): con `min_amount = 0`, el agente no crea pagos de monto 0 (`E24`), con saldo o sin el; el orden es E15 ->
/// E24 -> E05.
#[test]
fn p3_2_sin_pagos_de_monto_cero() {
    let mut c = new_contract();
    ctx(OWNER, 1, NOW);
    let mut i = operator_input();
    i.min_amount = U128(0);
    c.register_operator(OP.into(), i);
    ctx(PAYER, NearToken::from_millinear(50).as_yoctonear(), NOW);
    c.storage_deposit(None, None);
    deposit(&mut c, PAYER, AGENT, USDC_UNIT, (USDC_UNIT, USDC_UNIT));
    afb_falla(&mut c, AGENT, NOW, PAYER, &afb(0, 1), "ARRAS_E24");
    ctx(PAYER, 0, NOW);
    c.withdraw_budget(acc(AGENT), OP.into());
    afb_falla(&mut c, AGENT, NOW, PAYER, &afb(0, 2), "ARRAS_E24");
    let mut a = afb(0, 3);
    a.receiver = PAYER.into();
    afb_falla(&mut c, AGENT, NOW, PAYER, &a, "ARRAS_E15");
    // E24 antes que E05 (con un minimo).
    c.operators.get_mut(OP).unwrap().min_amount = U128(MIN_AMOUNT);
    afb_falla(&mut c, AGENT, NOW, PAYER, &afb(0, 4), "ARRAS_E24");
    assert_invariante(&c, &TODAS);
}

/// P2-1: el registro de un presupuesto patrocinado cobra exactamente lo que escribe, incluida la entrada nueva de
/// `sponsored_used` (sin ella, la pagaria la reserva libre del contrato).
#[test]
fn p2_1_storage_exacto_del_deposito_patrocinado() {
    let mut c = setup();
    ctx(OWNER, near(1).as_yoctonear(), NOW);
    c.fund_operator_storage(OP.into());
    let m = budget_msg(AGENT, USDC_UNIT, USDC_UNIT).to_canonical_json();
    ctx(USDC, 0, NOW);
    let antes = snapshot(&mut c, OTHER).storage_usage;
    assert_eq!(
        call_ft_on_transfer(&mut c, OTHER, USDC_UNIT, &m),
        0,
        "{:?}",
        logs()
    );
    let despues = snapshot(&mut c, OTHER).storage_usage;
    let b = c.budget_of(OTHER, AGENT, OP).unwrap();
    assert!(b.sponsored);
    assert_eq!(
        u128::from(despues - antes) * env::storage_byte_cost().as_yoctonear(),
        b.storage_charged.0,
        "lo cobrado es lo escrito"
    );
    assert_eq!(c.sponsored_used_of(OP, &acc(OTHER)), b.storage_charged.0);
    // Y el segundo presupuesto patrocinado del mismo owner (la entrada de sponsored_used ya existe) tambien.
    let m = budget_msg("agent-dos.testnet", USDC_UNIT, USDC_UNIT).to_canonical_json();
    let antes = snapshot(&mut c, OTHER).storage_usage;
    assert_eq!(
        call_ft_on_transfer(&mut c, OTHER, USDC_UNIT, &m),
        0,
        "{:?}",
        logs()
    );
    let despues = snapshot(&mut c, OTHER).storage_usage;
    let b2 = c.budget_of(OTHER, "agent-dos.testnet", OP).unwrap();
    assert_eq!(
        u128::from(despues - antes) * env::storage_byte_cost().as_yoctonear(),
        b2.storage_charged.0
    );
    assert!(b2.storage_charged.0 < b.storage_charged.0);
}

/// P3-5: `get_storage_health` no cuenta como libre la reserva de disputa cobrada y sin usar; al abrir una disputa la
/// reserva pasa a estar ocupada, y al compactar un pago sin disputa vuelve al payer.
#[test]
fn p3_5_storage_health_resta_la_reserva_de_disputa() {
    let mut c = setup();
    let reserva = bytes_cost(DISPUTE_RESERVE_BYTES);
    let h0 = c.get_storage_health();
    assert_eq!(h0.dispute_reserve, U128(0));
    let id = authorize(&mut c, PAYER, USDC_UNIT, &msg_salt(1));
    let id2 = authorize(&mut c, PAYER, USDC_UNIT, &msg_salt(2));
    let h = c.get_storage_health();
    assert_eq!(h.dispute_reserve, U128(2 * reserva));
    // Con la reserva viva, `free_reserve` la descuenta (mutante M53).
    assert!(h.free_reserve.0 > 0);
    assert_eq!(
        h.free_reserve.0 + 2 * reserva,
        h.balance.0 - h.storage_cost.0 - h.nep145_available.0 - h.sponsor_pools.0
    );
    ctx(PAYER, 0, NOW);
    c.open_dispute(id, "ab".repeat(32));
    assert_eq!(
        c.get_storage_health().dispute_reserve,
        U128(reserva + bytes_cost(DISPUTE_RESOLVE_BYTES))
    );
    ctx(CAPTURER, 0, NOW);
    c.refund(id2.clone());
    ctx(OTHER, 0, RECLAIM + RETENTION_NS);
    c.compact(id2);
    let h = c.get_storage_health();
    // Queda la parte de la disputa abierta que todavia no se uso (los 8 bytes de `resolved_at`).
    assert_eq!(h.dispute_reserve, U128(bytes_cost(DISPUTE_RESOLVE_BYTES)));
    assert_eq!(
        h.free_reserve.0,
        h.balance
            .0
            .saturating_sub(h.storage_cost.0)
            .saturating_sub(h.nep145_available.0)
            .saturating_sub(h.sponsor_pools.0)
            .saturating_sub(h.dispute_reserve.0)
    );
    assert_invariante(&c, &TODAS);
}

/// Mutante M24 (vivo en la primera corrida): el borde del saldo. Un pago de exactamente el saldo pasa y lo deja en 0;
/// uno de saldo + 1 es `E50` con el estado intacto.
#[test]
fn m24_borde_del_saldo() {
    let mut c = setup();
    deposit(
        &mut c,
        PAYER,
        AGENT,
        1_500_000,
        (2 * USDC_UNIT, 3 * USDC_UNIT),
    );
    afb_falla(&mut c, AGENT, NOW, PAYER, &afb(1_500_001, 1), "ARRAS_E50");
    ctx(AGENT, 0, NOW);
    call_afb(&mut c, PAYER, &afb(1_500_000, 1));
    assert_eq!(c.budget_of(PAYER, AGENT, OP).unwrap().saldo.0, 0);
    afb_falla(&mut c, AGENT, NOW, PAYER, &afb(MIN_AMOUNT, 2), "ARRAS_E50");
    assert_invariante(&c, &TODAS);
}

/// Ronda 2, P3-N2: la llave publica de los vectores (semilla publicada) nunca es una `ticket_pubkey` (`E48`).
#[test]
fn r2_n2_la_llave_de_los_vectores_no_es_ticket_pubkey() {
    let vectores = ed25519_dalek::SigningKey::from_bytes(&[1; 32])
        .verifying_key()
        .to_bytes();
    assert_eq!(vectores, crate::admin::LLAVE_DE_LOS_VECTORES);
    let pk = PublicKey::from_parts(near_sdk::CurveType::ED25519, vectores.to_vec()).unwrap();
    let mut c = setup();
    ctx(OWNER, 1, NOW);
    let mut i = operator_input();
    i.ticket_pubkey = Some(pk.clone());
    expect_panic("ARRAS_E48", || c.register_operator("kk".into(), i));
    expect_panic("ARRAS_E48", || {
        c.update_operator(
            OP.into(),
            OperatorPatch {
                ticket_pubkey: Some(pk),
                ..Default::default()
            },
        )
    });
}

/// Ronda 2, P2-N1: una tarea de EM de 30 dias (`reclaim_after = deadline + 7 d de revision + 7 d de reembolso`,
/// `now + 44 dias`) entra por el presupuesto; el horizonte es 60 dias.
#[test]
fn r2_n1_una_tarea_de_em_de_30_dias_entra_por_el_presupuesto() {
    let mut c = setup();
    deposit(
        &mut c,
        PAYER,
        AGENT,
        5 * USDC_UNIT,
        (2 * USDC_UNIT, 3 * USDC_UNIT),
    );
    let semana = 7 * DAY_NS;
    let deadline = NOW + 30 * DAY_NS;
    let mut a = afb(USDC_UNIT, 1);
    a.release_deadline = deadline + semana;
    a.reclaim_after = deadline + 2 * semana;
    ctx(AGENT, 0, NOW);
    let id = call_afb(&mut c, PAYER, &a);
    assert_eq!(
        c.get_payment(id).unwrap().reclaim_after,
        U64(NOW + 44 * DAY_NS)
    );
    const { assert!(44 * DAY_NS < MAX_PLAZO_PRESUPUESTO_NS) };
}

/// Ronda 2, P3-N1: la reserva baja 78 bytes al abrir la disputa y 8 al resolverla (lo que crece el `Payment` en cada
/// paso), asi `free_reserve` no cuenta de mas mientras la disputa esta abierta.
#[test]
fn r2_n3_reserva_exacta_con_la_disputa_abierta() {
    let mut c = setup();
    let id = authorize(&mut c, PAYER, USDC_UNIT, &msg_salt(1));
    ctx(PAYER, 0, NOW);
    c.open_dispute(id.clone(), "ab".repeat(32));
    assert_eq!(c.dispute_reserve_total, bytes_cost(DISPUTE_RESOLVE_BYTES));
    ctx(ARBITER, 0, NOW);
    c.resolve_dispute(id, U128(USDC_UNIT), U128(0));
    assert_eq!(c.dispute_reserve_total, 0);
    assert_invariante(&c, &TODAS);
}

// Refutacion de c0der (RARRAS-6, ronda 1), sobre 179a843.

const RAZON: &str = "1111111111111111111111111111111111111111111111111111111111111111";

/// Abre la disputa de `id` como `quien` en `ts`; el arbitro no contesta y un tercero la cierra al vencer su plazo.
fn disputa_vencida(c: &mut Arras, quien: &str, ts: u64, id: &PaymentId) {
    ctx(quien, 0, ts);
    c.open_dispute(id.clone(), RAZON.into());
    ctx(OTHER, 0, ts + ARBITER_WINDOW - 1);
    expect_panic("ARRAS_E64", || c.resolve_expired_dispute(id.clone()));
    ctx(OTHER, 0, ts + ARBITER_WINDOW);
    c.resolve_expired_dispute(id.clone());
}

/// P2-1: el payer disputa en o despues de `release_deadline` (el capturer ya no podia liberar, `E23`) y el arbitro no
/// contesta: lo vivo vuelve al payer, el ultimo estado acordado. Sin release previo, el payer recupera el 100 % y el
/// worker 0, lo mismo que con `reclaim` sin disputa; con un release parcial previo, lo liberado queda liberado.
#[test]
fn rarras6_p2_1_el_payer_disputa_desde_release_deadline_y_recupera_lo_vivo() {
    for abre in [RELEASE, RELEASE + 1] {
        let mut c = setup();
        let id = authorize(&mut c, PAYER, USDC_UNIT, &msg_v1().to_canonical_json());
        ctx(CAPTURER, 0, abre);
        expect_panic("ARRAS_E23", || c.release(id.clone(), None));
        disputa_vencida(&mut c, PAYER, abre, &id);
        assert_eq!(
            logs(),
            vec![format!(
                r#"EVENT_JSON:{{"standard":"arras","version":"1.0.0","event":"dispute_resolved","data":[{{"payment_id":"{id}","by":"{OTHER}","to_receiver":"0","net":"0","fee":"0","to_payer":"1000000","expired":true}}]}}"#
            )],
            "abre en {abre}"
        );
        let p = c.get_payment(id).unwrap();
        assert_eq!(
            (p.released.0, p.refunded.0, p.status),
            (0, USDC_UNIT, PaymentStatus::Resolved)
        );
        assert_eq!(c.claimable_of(&acc(PAYER)), USDC_UNIT, "abre en {abre}");
        assert_eq!(c.claimable_of(&acc(WORKER)), 0, "abre en {abre}");
        assert_eq!(c.claimable_of(&acc(FEES)), 0, "abre en {abre}");
        assert_invariante(&c, &TODAS);
    }
    // Con un release parcial antes: lo liberado queda liberado y lo vivo vuelve al payer.
    let mut c = setup();
    let id = authorize(&mut c, PAYER, USDC_UNIT, &msg_v1().to_canonical_json());
    ctx(CAPTURER, 0, NOW);
    c.release(id.clone(), Some(U128(400_000)));
    disputa_vencida(&mut c, PAYER, RELEASE, &id);
    let p = c.get_payment(id).unwrap();
    assert_eq!((p.released.0, p.refunded.0), (400_000, 600_000));
    assert_eq!(c.claimable_of(&acc(PAYER)), 600_000);
    assert_eq!(c.claimable_of(&acc(WORKER)), 348_000);
    assert_eq!(c.claimable_of(&acc(FEES)), 52_000);
    assert_invariante(&c, &TODAS);
}

/// P2-1, el borde de abajo: abierta por el payer en `release_deadline - 1` (el capturer todavia podia liberar), la
/// regla de la sec. 4 sigue igual: lo vivo va al receiver, con la tarifa.
#[test]
fn rarras6_p2_1_el_payer_disputa_antes_de_release_deadline_y_cobra_el_receiver() {
    let mut c = setup();
    let id = authorize(&mut c, PAYER, USDC_UNIT, &msg_v1().to_canonical_json());
    disputa_vencida(&mut c, PAYER, RELEASE - 1, &id);
    let p = c.get_payment(id).unwrap();
    assert_eq!((p.released.0, p.refunded.0), (USDC_UNIT, 0));
    assert_eq!(c.claimable_of(&acc(PAYER)), 0);
    assert_eq!(c.claimable_of(&acc(WORKER)), 870_000);
    assert_eq!(c.claimable_of(&acc(FEES)), 130_000);
    assert_invariante(&c, &TODAS);
}

/// P2-1, la llave del agente filtrada: el atacante autoriza desde el presupuesto a una cuenta suya (OTHER) y el capturer
/// nunca libera. Si el owner disputa desde `release_deadline` y el arbitro no contesta, recupera todo; si disputa
/// antes, cobra el receiver (por eso el README le dice que espere el `reclaim`, pida el refund o dispute desde
/// `release_deadline`). Sin disputa, `reclaim` le devuelve todo.
#[test]
fn rarras6_p2_1_llave_del_agente_filtrada() {
    let mut c = setup();
    deposit(
        &mut c,
        PAYER,
        AGENT,
        3 * USDC_UNIT,
        (USDC_UNIT, 3 * USDC_UNIT),
    );
    let mut ids = vec![];
    for n in [7, 8, 9] {
        let mut a = afb(USDC_UNIT, n);
        a.receiver = OTHER.into();
        ctx(AGENT, 0, NOW);
        ids.push(call_afb(&mut c, PAYER, &a));
    }
    // En orden de tiempo: el owner disputa uno antes de release_deadline y otro desde release_deadline, reclama el
    // tercero sin disputa, y el arbitro no contesta ninguna.
    ctx(PAYER, 0, NOW + 1);
    c.open_dispute(ids[1].clone(), RAZON.into());
    ctx(PAYER, 0, RELEASE);
    c.open_dispute(ids[0].clone(), RAZON.into());
    ctx(PAYER, 0, RECLAIM);
    c.reclaim(ids[2].clone());
    assert_eq!(
        c.claimable_of(&acc(PAYER)),
        USDC_UNIT,
        "sin disputa: reclaim"
    );
    // La de antes: cobra el atacante.
    ctx(OTHER, 0, NOW + 1 + ARBITER_WINDOW);
    c.resolve_expired_dispute(ids[1].clone());
    assert_eq!(c.claimable_of(&acc(OTHER)), 870_000);
    // La de desde release_deadline: vuelve al owner.
    ctx(OTHER, 0, RELEASE + ARBITER_WINDOW);
    c.resolve_expired_dispute(ids[0].clone());
    assert_eq!(c.claimable_of(&acc(PAYER)), 2 * USDC_UNIT);
    assert_eq!(c.claimable_of(&acc(OTHER)), 870_000);
    assert_invariante(&c, &TODAS);
}

/// P2-1: si la abre el receiver, lo vivo vuelve al payer antes y despues de `release_deadline` (sin cambios).
#[test]
fn rarras6_p2_1_la_abre_el_receiver_vuelve_al_payer_siempre() {
    for abre in [RELEASE - 1, RELEASE, RELEASE + 1] {
        let mut c = setup();
        let id = authorize(&mut c, PAYER, USDC_UNIT, &msg_v1().to_canonical_json());
        disputa_vencida(&mut c, WORKER, abre, &id);
        assert_eq!(c.claimable_of(&acc(PAYER)), USDC_UNIT, "abre en {abre}");
        assert_eq!(c.claimable_of(&acc(WORKER)), 0, "abre en {abre}");
        assert_invariante(&c, &TODAS);
    }
}

/// P3-1: el agente no le cierra la disputa al owner: `dispute_deadline >= release_deadline` (`E67`, con el estado
/// intacto), y con el `dispute_deadline` mas corto que puede elegir el owner todavia disputa hasta `release_deadline`.
#[test]
fn rarras6_p3_1_el_agente_no_cierra_la_disputa_antes_de_release_deadline() {
    let mut c = setup();
    deposit(
        &mut c,
        PAYER,
        AGENT,
        2 * USDC_UNIT,
        (USDC_UNIT, 2 * USDC_UNIT),
    );
    for d in [NOW + 1, RELEASE - 1] {
        let mut a = afb(USDC_UNIT, 8);
        a.dispute_deadline = Some(d);
        afb_falla(&mut c, AGENT, NOW, PAYER, &a, "ARRAS_E67");
    }
    let mut ids = vec![];
    for n in [8, 9] {
        let mut a = afb(USDC_UNIT, n);
        a.dispute_deadline = Some(RELEASE);
        ctx(AGENT, 0, NOW);
        ids.push(call_afb(&mut c, PAYER, &a));
    }
    ctx(PAYER, 0, RELEASE - 1);
    c.open_dispute(ids[0].clone(), RAZON.into());
    assert_eq!(
        c.get_payment(ids[0].clone()).unwrap().status,
        PaymentStatus::Disputed
    );
    ctx(PAYER, 0, RELEASE);
    expect_panic("ARRAS_E61", || c.open_dispute(ids[1].clone(), RAZON.into()));
    assert_invariante(&c, &TODAS);
}

/// P3-2 (declarado, sin cambio de codigo): el ticket de presupuesto firma `(operator_id, owner, agent)` y `expiry`, sin
/// monto ni nonce: vale para cualquier cantidad de depositos del par hasta `expiry` (tambien despues de `revoke_agent`),
/// y en `expiry` ya no (`E54`). Los firmantes usan `expiry` corto.
#[test]
fn rarras6_p3_2_el_ticket_de_presupuesto_vale_hasta_expiry() {
    let mut c = setup();
    con_ticket(&mut c);
    let expiry = NOW + DAY_NS;
    let t = firmar_ticket(
        crate::ticket::TicketSubject::Budget {
            operator_id: OP,
            owner: PAYER,
            agent: AGENT,
        },
        expiry,
    );
    let mut m = budget_msg(AGENT, USDC_UNIT, 2 * USDC_UNIT);
    m.operator_ticket = Some(crate::msg::OperatorTicket {
        expiry: t.expiry.0,
        sig: t.sig.clone(),
    });
    let json = m.to_canonical_json();
    for vuelta in 0..3 {
        assert_eq!(
            ft_on_transfer_as(&mut c, USDC, PAYER, USDC_UNIT, &json),
            0,
            "vuelta {vuelta}"
        );
        ctx(PAYER, 0, NOW);
        c.revoke_agent(acc(AGENT), OP.into());
    }
    ctx(USDC, 0, expiry);
    assert_eq!(
        call_ft_on_transfer(&mut c, PAYER, USDC_UNIT, &json),
        USDC_UNIT
    );
    assert_eq!(logs(), vec!["arras:rechazo:E54_TICKET_VENCIDO".to_string()]);
    assert_invariante(&c, &TODAS);
}
