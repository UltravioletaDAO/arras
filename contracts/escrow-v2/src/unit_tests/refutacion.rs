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

/// P1-1 (R-1): el agente no puede trabar la plata del owner con plazos lejanos: `reclaim_after <= now + 30 dias`
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
        a.dispute_deadline = Some(NOW + 1);
        afb_falla(&mut c, AGENT, NOW, PAYER, &a, "ARRAS_E57");
    }
    // En el borde exacto pasa, y el owner reclama en `reclaim_after` aunque el agente no le haya dejado disputar.
    let mut a = afb(2 * USDC_UNIT, 1);
    a.receiver = OTHER.into();
    a.release_deadline = limite - 1;
    a.reclaim_after = limite;
    a.dispute_deadline = Some(NOW + 1);
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
    assert_eq!(c.get_storage_health().dispute_reserve, U128(2 * reserva));
    ctx(PAYER, 0, NOW);
    c.open_dispute(id, "ab".repeat(32));
    assert_eq!(c.get_storage_health().dispute_reserve, U128(reserva));
    ctx(CAPTURER, 0, NOW);
    c.refund(id2.clone());
    ctx(OTHER, 0, RECLAIM + RETENTION_NS);
    c.compact(id2);
    let h = c.get_storage_health();
    assert_eq!(h.dispute_reserve, U128(0));
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
