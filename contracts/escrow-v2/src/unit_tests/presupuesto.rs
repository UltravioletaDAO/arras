//! Presupuesto de agente: deposito (`msg` `budget`), `authorize_from_budget`, `withdraw_budget`, `revoke_agent` y
//! `set_budget_caps`. Los rechazos dejan el estado intacto (snapshot completo).

use super::common::*;
use crate::*;

const TOPES: (u128, u128) = (2 * USDC_UNIT, 3 * USDC_UNIT);
const KEY_BYTES: u64 = 1 + 4 + PAYER.len() as u64 + 4 + AGENT.len() as u64 + 4 + 2;

/// Presupuesto de 5 USDC de PAYER para AGENT, tope 2 por pago y 3 por dia.
fn con_presupuesto() -> Arras {
    let mut c = setup();
    deposit(&mut c, PAYER, AGENT, 5 * USDC_UNIT, TOPES);
    c
}

/// Rechazo de un deposito: `Value(amount)`, el log y el estado intacto.
fn rechazo_deposito(c: &mut Arras, token: &str, owner: &str, amount: u128, msg: &str, code: &str) {
    assert_rechazo(c, token, owner, amount, msg, code);
}

/// `authorize_from_budget` que tiene que fallar con `needle`, sin cambiar nada.
fn afb_falla(c: &mut Arras, agent: &str, ts: u64, owner: &str, a: &Afb, needle: &str) {
    ctx(agent, 0, ts);
    let antes = snapshot(c, owner);
    expect_panic(needle, || call_afb(c, owner, a));
    assert_eq!(snapshot(c, owner), antes, "estado intacto tras {needle}");
}

#[test]
fn deposito_crea_el_presupuesto_y_sube_los_tres_tvl() {
    let mut c = setup();
    ctx(USDC, 0, NOW);
    let antes = snapshot(&mut c, PAYER);
    let m = budget_msg(AGENT, TOPES.0, TOPES.1).to_canonical_json();
    assert_eq!(call_ft_on_transfer(&mut c, PAYER, 5 * USDC_UNIT, &m), 0);
    let l = logs();
    let despues = snapshot(&mut c, PAYER);
    let b = c.budget_of(PAYER, AGENT, OP).unwrap();
    assert_eq!(
        b,
        Budget {
            saldo: U128(5 * USDC_UNIT),
            tope_por_pago: U128(TOPES.0),
            tope_diario: U128(TOPES.1),
            gastado_hoy: U128(0),
            dia: U64(NOW / DAY_NS),
            storage_charged: b.storage_charged,
            sponsored: false,
        }
    );
    assert_eq!(c.tvl_global, 5 * USDC_UNIT);
    assert_eq!(c.budgets_total, 5 * USDC_UNIT);
    assert_eq!(
        c.get_operator(OP.into()).unwrap().tvl_operador.0,
        5 * USDC_UNIT
    );
    assert_eq!(c.payer_tvl_of(OP, &acc(PAYER)), 5 * USDC_UNIT);
    assert_eq!(c.payments.len(), 0, "un deposito no es un pago");
    // Storage exacto: el registro del presupuesto, payer_tvl y claimable[owner], del NEP-145 del owner.
    let bytes = despues.storage_usage - antes.storage_usage;
    assert_eq!(
        u128::from(bytes) * env::storage_byte_cost().as_yoctonear(),
        b.storage_charged.0
    );
    assert_eq!(
        bytes,
        record_bytes(KEY_BYTES, crate::storage::BUDGET_VALUE_BYTES)
            + record_bytes(1 + 4 + 2 + 4 + PAYER.len() as u64, 16)
            + record_bytes(1 + 4 + PAYER.len() as u64, 16)
    );
    assert_eq!(
        despues.storage_payer.unwrap().1,
        antes.storage_payer.unwrap().1 - b.storage_charged.0
    );
    assert_eq!(c.claimable.get(&acc(PAYER)).copied(), Some(0));
    assert_eq!(
        l,
        vec![format!(
            r#"EVENT_JSON:{{"standard":"arras","version":"1.0.0","event":"budget_deposited","data":[{{"owner":"{PAYER}","agent":"{AGENT}","operator_id":"em","amount":"5000000","saldo":"5000000","tope_por_pago":"2000000","tope_diario":"3000000"}}]}}"#
        )]
    );
    assert_invariante(&c, &TODAS);
}

#[test]
fn redeposito_suma_saldo_y_solo_baja_los_topes() {
    let mut c = con_presupuesto();
    let storage = c.storage.get(&acc(PAYER)).copied();
    deposit(
        &mut c,
        PAYER,
        AGENT,
        USDC_UNIT,
        (9 * USDC_UNIT, 9 * USDC_UNIT),
    );
    let b = c.budget_of(PAYER, AGENT, OP).unwrap();
    assert_eq!(b.saldo.0, 6 * USDC_UNIT);
    assert_eq!((b.tope_por_pago.0, b.tope_diario.0), TOPES, "subir no sube");
    assert_eq!(
        c.storage.get(&acc(PAYER)).copied(),
        storage,
        "sin storage nuevo"
    );
    deposit(&mut c, PAYER, AGENT, USDC_UNIT, (USDC_UNIT, 9 * USDC_UNIT));
    let b = c.budget_of(PAYER, AGENT, OP).unwrap();
    assert_eq!((b.tope_por_pago.0, b.tope_diario.0), (USDC_UNIT, TOPES.1));
    assert_eq!(c.tvl_global, 7 * USDC_UNIT);
    assert_invariante(&c, &TODAS);
}

#[test]
fn deposito_guardas() {
    let mut c = setup();
    let ok = budget_msg(AGENT, TOPES.0, TOPES.1);
    let m = ok.to_canonical_json();
    rechazo_deposito(
        &mut c,
        OTHER,
        PAYER,
        USDC_UNIT,
        &m,
        "E01_TOKEN_NO_PERMITIDO",
    );
    c.paused = true;
    rechazo_deposito(&mut c, USDC, PAYER, USDC_UNIT, &m, "E02_PAUSADO");
    c.paused = false;
    for malo in [
        m.replace("\"v\":1", "\"v\":2"),
        m.replace("\"tope_diario\":\"3000000\"", "\"tope_diario\":3000000"),
        m.replace('}', ",\"receiver\":\"x.testnet\"}"),
        m.replace(",\"agent\":\"agent-demo.testnet\"", ""),
    ] {
        rechazo_deposito(&mut c, USDC, PAYER, USDC_UNIT, &malo, "E03_MSG_INVALIDO");
    }
    let mut x = ok.clone();
    x.operator_id = "kk".into();
    rechazo_deposito(
        &mut c,
        USDC,
        PAYER,
        USDC_UNIT,
        &x.to_canonical_json(),
        "E04_OPERADOR_INEXISTENTE",
    );
    let mut x = ok.clone();
    x.agent = acc(CONTRACT);
    rechazo_deposito(
        &mut c,
        USDC,
        PAYER,
        USDC_UNIT,
        &x.to_canonical_json(),
        "E55_AGENTE_INVALIDO",
    );
    for (pp, pd) in [(0, 1), (1, 0), (0, 0)] {
        let x = budget_msg(AGENT, pp, pd);
        rechazo_deposito(
            &mut c,
            USDC,
            PAYER,
            USDC_UNIT,
            &x.to_canonical_json(),
            "E56_TOPES_PRESUPUESTO",
        );
    }
    rechazo_deposito(
        &mut c,
        USDC,
        PAYER,
        MIN_AMOUNT - 1,
        &m,
        "E05_DEBAJO_DEL_MINIMO",
    );
    // E08 al tope por payer (20 USDC); 20 exactos pasan.
    rechazo_deposito(
        &mut c,
        USDC,
        PAYER,
        TOPE_TVL_POR_PAYER + 1,
        &m,
        "E08_TVL_PAYER",
    );
    rechazo_deposito(&mut c, USDC, PAYER, u128::MAX, &m, "E08_TVL_PAYER");
    c.operators.get_mut(OP).unwrap().tope_tvl_operador = U128(USDC_UNIT);
    rechazo_deposito(&mut c, USDC, PAYER, USDC_UNIT + 1, &m, "E09_TVL_OPERADOR");
    c.operators.get_mut(OP).unwrap().tope_tvl_operador = U128(TOPE_TVL_OPERADOR);
    c.tope_tvl_global = USDC_UNIT;
    rechazo_deposito(&mut c, USDC, PAYER, USDC_UNIT + 1, &m, "E10_TVL_GLOBAL");
    c.tope_tvl_global = TOPE_TVL_GLOBAL;
    // E14: un owner sin NEP-145 y sin patrocinio.
    rechazo_deposito(&mut c, USDC, OTHER, USDC_UNIT, &m, "E14_SIN_STORAGE");
    // El tope por pago global (E06) no aplica al deposito: el deposito no es un pago.
    deposit(&mut c, PAYER, AGENT, TOPE_TVL_POR_PAYER, TOPES);
    const { assert!(TOPE_TVL_POR_PAYER > TOPE_POR_PAGO) };
    assert_invariante(&c, &TODAS);
}

/// La primera guarda que falla decide (pares consecutivos).
#[test]
fn deposito_orden_de_las_guardas() {
    let mut c = setup();
    // E04 antes que E55.
    let mut x = budget_msg(CONTRACT, TOPES.0, TOPES.1);
    x.operator_id = "kk".into();
    rechazo_deposito(
        &mut c,
        USDC,
        PAYER,
        USDC_UNIT,
        &x.to_canonical_json(),
        "E04_OPERADOR_INEXISTENTE",
    );
    // E55 antes que E56.
    let x = budget_msg(CONTRACT, 0, 0);
    rechazo_deposito(
        &mut c,
        USDC,
        PAYER,
        USDC_UNIT,
        &x.to_canonical_json(),
        "E55_AGENTE_INVALIDO",
    );
    // E56 antes que E05.
    let x = budget_msg(AGENT, 0, 0);
    rechazo_deposito(
        &mut c,
        USDC,
        PAYER,
        1,
        &x.to_canonical_json(),
        "E56_TOPES_PRESUPUESTO",
    );
    // E05 antes que E08 (con min_amount por encima del tope por payer).
    let m = budget_msg(AGENT, TOPES.0, TOPES.1).to_canonical_json();
    c.operators.get_mut(OP).unwrap().min_amount = U128(TOPE_TVL_POR_PAYER + 2);
    rechazo_deposito(
        &mut c,
        USDC,
        PAYER,
        TOPE_TVL_POR_PAYER + 1,
        &m,
        "E05_DEBAJO_DEL_MINIMO",
    );
    c.operators.get_mut(OP).unwrap().min_amount = U128(MIN_AMOUNT);
    // E10 antes que E14 (owner sin storage y el global lleno).
    c.tope_tvl_global = 0;
    rechazo_deposito(&mut c, USDC, OTHER, USDC_UNIT, &m, "E10_TVL_GLOBAL");
}

#[test]
fn authorize_from_budget_ok() {
    let mut c = con_presupuesto();
    let a = afb(USDC_UNIT, 1);
    let id_esperado = afb_id(PAYER, &a);
    ctx(AGENT, 0, NOW);
    let antes = snapshot(&mut c, PAYER);
    let id = call_afb(&mut c, PAYER, &a);
    let l = logs();
    let despues = snapshot(&mut c, PAYER);
    assert_eq!(id, id_esperado);
    let p = c.get_payment(id.clone()).unwrap();
    assert_eq!(
        p.payer,
        acc(PAYER),
        "el payer del pago es el owner, no el agente"
    );
    assert_eq!(p.receiver, acc(WORKER));
    assert_eq!(p.amount, U128(USDC_UNIT));
    assert_eq!(p.status, PaymentStatus::Authorized);
    assert_eq!(
        p.dispute_deadline,
        U64(RECLAIM),
        "sin dispute_deadline: reclaim_after"
    );
    assert_eq!(p.arbiter_window_ns, U64(ARBITER_WINDOW));
    let b = c.budget_of(PAYER, AGENT, OP).unwrap();
    assert_eq!(
        (b.saldo.0, b.gastado_hoy.0, b.dia.0),
        (4 * USDC_UNIT, USDC_UNIT, NOW / DAY_NS)
    );
    // Los TVL no se mueven: la plata ya estaba contada en el presupuesto.
    assert_eq!(
        (despues.tvl_global, despues.tvl_operador, despues.payer_tvl),
        (antes.tvl_global, antes.tvl_operador, antes.payer_tvl)
    );
    assert_eq!(c.budgets_total, 4 * USDC_UNIT);
    // Storage del owner: lo que escribe el pago mas la reserva de la disputa.
    let bytes = despues.storage_usage - antes.storage_usage;
    assert_eq!(
        u128::from(bytes + DISPUTE_RESERVE_BYTES) * env::storage_byte_cost().as_yoctonear(),
        p.storage_charged.0
    );
    assert_eq!(
        despues.storage_payer.unwrap().1,
        antes.storage_payer.unwrap().1 - p.storage_charged.0
    );
    assert_eq!(l.len(), 2);
    assert!(
        l[0].contains(r#""event":"authorized""#) && l[0].contains(&format!(r#""payer":"{PAYER}""#)),
        "{}",
        l[0]
    );
    assert_eq!(
        l[1],
        format!(
            r#"EVENT_JSON:{{"standard":"arras","version":"1.0.0","event":"budget_spent","data":[{{"owner":"{PAYER}","agent":"{AGENT}","operator_id":"em","payment_id":"{id}","amount":"1000000","saldo":"4000000","gastado_hoy":"1000000"}}]}}"#
        )
    );
    // La vista calcula el mismo id.
    assert_eq!(
        c.compute_budget_payment_id(
            acc(PAYER),
            OP.into(),
            acc(WORKER),
            U128(USDC_UNIT),
            U64(RELEASE),
            U64(RECLAIM),
            1300,
            "0".repeat(64),
            a.salt.clone()
        ),
        id
    );
    assert_invariante(&c, &TODAS);
}

#[test]
fn authorize_from_budget_topes_y_ventana_diaria() {
    let mut c = con_presupuesto();
    // Por pago: 2 USDC pasa, 2 USDC + 1 es E52; el global (10) es E06 antes que E52.
    afb_falla(&mut c, AGENT, NOW, PAYER, &afb(TOPES.0 + 1, 1), "ARRAS_E52");
    afb_falla(
        &mut c,
        AGENT,
        NOW,
        PAYER,
        &afb(TOPE_POR_PAGO + 1, 1),
        "ARRAS_E06",
    );
    ctx(AGENT, 0, NOW);
    call_afb(&mut c, PAYER, &afb(TOPES.0, 1));
    // Diario: 2 + 1 = 3 pasa, 2 + 1 + 1 no (E51).
    ctx(AGENT, 0, NOW + 10);
    call_afb(&mut c, PAYER, &afb(USDC_UNIT, 2));
    afb_falla(
        &mut c,
        AGENT,
        NOW + 20,
        PAYER,
        &afb(MIN_AMOUNT, 3),
        "ARRAS_E51",
    );
    // Al dia UTC siguiente la ventana se reinicia (saldo: 5 - 3 = 2).
    let manana = (NOW / DAY_NS + 1) * DAY_NS;
    let mut a = afb(TOPES.0, 4);
    a.release_deadline = manana + DAY_NS;
    a.reclaim_after = manana + 2 * DAY_NS;
    afb_falla(&mut c, AGENT, manana - 1, PAYER, &a, "ARRAS_E51");
    ctx(AGENT, 0, manana);
    call_afb(&mut c, PAYER, &a);
    let b = c.budget_of(PAYER, AGENT, OP).unwrap();
    assert_eq!(
        (b.saldo.0, b.gastado_hoy.0, b.dia.0),
        (0, TOPES.0, manana / DAY_NS)
    );
    // Sin saldo: E50 aunque el tope diario alcance.
    let mut a = afb(MIN_AMOUNT, 5);
    a.release_deadline = manana + DAY_NS;
    a.reclaim_after = manana + 2 * DAY_NS;
    afb_falla(&mut c, AGENT, manana, PAYER, &a, "ARRAS_E50");
    assert_invariante(&c, &TODAS);
}

#[test]
fn authorize_from_budget_guardas() {
    let mut c = con_presupuesto();
    let a = afb(USDC_UNIT, 1);
    // Solo el agente de ese presupuesto: el owner, un tercero o el agente con otro owner -> E50.
    afb_falla(&mut c, PAYER, NOW, PAYER, &a, "ARRAS_E50");
    afb_falla(&mut c, OTHER, NOW, PAYER, &a, "ARRAS_E50");
    afb_falla(&mut c, AGENT, NOW, OTHER, &a, "ARRAS_E50");
    // E02 en pausa.
    c.paused = true;
    afb_falla(&mut c, AGENT, NOW, PAYER, &a, "ARRAS_E02");
    c.paused = false;
    // E03: hex invalido.
    for (t, s) in [
        ("F".repeat(64), a.salt.clone()),
        ("0".repeat(63), a.salt.clone()),
        ("0".repeat(64), "0".repeat(30)),
        ("0".repeat(64), format!("0x{}", "0".repeat(30))),
    ] {
        let mut x = a.clone();
        x.terms_hash = t;
        x.salt = s;
        afb_falla(&mut c, AGENT, NOW, PAYER, &x, "ARRAS_E03");
    }
    // E04.
    let mut x = a.clone();
    x.operator_id = "kk".into();
    afb_falla(&mut c, AGENT, NOW, PAYER, &x, "ARRAS_E04");
    // E15: receiver = contrato, owner o agente.
    for r in [CONTRACT, PAYER, AGENT] {
        let mut x = a.clone();
        x.receiver = r.into();
        afb_falla(&mut c, AGENT, NOW, PAYER, &x, "ARRAS_E15");
    }
    // E05.
    afb_falla(
        &mut c,
        AGENT,
        NOW,
        PAYER,
        &afb(MIN_AMOUNT - 1, 1),
        "ARRAS_E05",
    );
    // E11: plazos, con dispute_deadline.
    for (rel, rec, dis) in [
        (NOW, RECLAIM, None),
        (RELEASE, RELEASE, None),
        (RELEASE, RECLAIM, Some(RECLAIM + 1)),
        (RELEASE, RECLAIM, Some(NOW)),
    ] {
        let mut x = a.clone();
        x.release_deadline = rel;
        x.reclaim_after = rec;
        x.dispute_deadline = dis;
        afb_falla(&mut c, AGENT, NOW, PAYER, &x, "ARRAS_E11");
    }
    // E67: dispute_deadline antes de release_deadline (RARRAS-6, P3-1), en el borde y lejos.
    for dis in [RELEASE - 1, NOW + 1] {
        let mut x = a.clone();
        x.dispute_deadline = Some(dis);
        afb_falla(&mut c, AGENT, NOW, PAYER, &x, "ARRAS_E67");
    }
    // E12.
    let mut x = a.clone();
    x.fee_bps_max = 1299;
    afb_falla(&mut c, AGENT, NOW, PAYER, &x, "ARRAS_E12");
    // E13: el mismo pago dos veces.
    ctx(AGENT, 0, NOW);
    call_afb(&mut c, PAYER, &a);
    afb_falla(&mut c, AGENT, NOW, PAYER, &a, "ARRAS_E13");
    // E14: el owner sin saldo NEP-145 y sin patrocinio.
    let disp = c.storage.get(&acc(PAYER)).unwrap().available;
    c.storage.get_mut(&acc(PAYER)).unwrap().available = 0;
    c.storage_available_total -= disp;
    afb_falla(&mut c, AGENT, NOW, PAYER, &afb(USDC_UNIT, 9), "ARRAS_E14");
    // Un ticket sin llave en el operador: E53 (nadie lo puede verificar).
    c.storage.get_mut(&acc(PAYER)).unwrap().available = disp;
    c.storage_available_total += disp;
    let mut x = afb(USDC_UNIT, 10);
    x.ticket = Some(TicketArg {
        expiry: U64(RELEASE),
        sig: "x".into(),
    });
    afb_falla(&mut c, AGENT, NOW, PAYER, &x, "ARRAS_E53");
    // dispute_deadline explicito, en los dos bordes: == reclaim_after y == release_deadline.
    let mut x = afb(USDC_UNIT, 11);
    x.dispute_deadline = Some(RECLAIM);
    ctx(AGENT, 0, NOW);
    let id = call_afb(&mut c, PAYER, &x);
    assert_eq!(c.get_payment(id).unwrap().dispute_deadline, U64(RECLAIM));
    let mut x = afb(USDC_UNIT, 12);
    x.dispute_deadline = Some(RELEASE);
    let id = call_afb(&mut c, PAYER, &x);
    assert_eq!(c.get_payment(id).unwrap().dispute_deadline, U64(RELEASE));
    assert_invariante(&c, &TODAS);
}

/// Orden: E02 -> E03 -> E04 -> E50 -> E15 -> E05 -> E06 -> E52 -> E51 -> E50 -> E11 -> E67 -> E12 -> E13 -> E53 -> E14. Cada
/// paso rompe dos guardas consecutivas y espera la primera.
#[test]
fn authorize_from_budget_orden_de_las_guardas() {
    let mut c = con_presupuesto();
    let mut x = afb(TOPE_POR_PAGO + 1, 1);
    x.terms_hash = "X".into();
    x.operator_id = "kk".into();
    x.receiver = PAYER.into();
    c.paused = true;
    afb_falla(&mut c, AGENT, NOW, PAYER, &x, "ARRAS_E02");
    c.paused = false;
    afb_falla(&mut c, AGENT, NOW, PAYER, &x, "ARRAS_E03");
    x.terms_hash = "0".repeat(64);
    afb_falla(&mut c, OTHER, NOW, PAYER, &x, "ARRAS_E04");
    x.operator_id = OP.into();
    afb_falla(&mut c, OTHER, NOW, PAYER, &x, "ARRAS_E50");
    let mut y = x.clone();
    y.amount = MIN_AMOUNT - 1;
    afb_falla(&mut c, AGENT, NOW, PAYER, &y, "ARRAS_E15");
    x.receiver = WORKER.into();
    // E05 antes que E06 (min_amount por encima del tope global).
    c.operators.get_mut(OP).unwrap().min_amount = U128(TOPE_POR_PAGO + 2);
    afb_falla(&mut c, AGENT, NOW, PAYER, &x, "ARRAS_E05");
    c.operators.get_mut(OP).unwrap().min_amount = U128(MIN_AMOUNT);
    // E06 antes que E52 (11 USDC pasa los dos topes).
    afb_falla(&mut c, AGENT, NOW, PAYER, &x, "ARRAS_E06");
    // E52 antes que E51: con el dia gastado (2 + 1 de 3), un pago de 2 + 1 rompe los dos.
    ctx(AGENT, 0, NOW);
    call_afb(&mut c, PAYER, &afb(TOPES.0, 20));
    call_afb(&mut c, PAYER, &afb(USDC_UNIT, 21));
    afb_falla(&mut c, AGENT, NOW, PAYER, &afb(TOPES.0 + 1, 1), "ARRAS_E52");
    // E51 antes que E50 (saldo): saldo 1 yocto y el dia gastado.
    c.budgets
        .get_mut(&(acc(PAYER), acc(AGENT), OP.to_string()))
        .unwrap()
        .saldo = U128(1);
    afb_falla(&mut c, AGENT, NOW, PAYER, &afb(USDC_UNIT, 1), "ARRAS_E51");
    // E50 (saldo) antes que E11: al dia siguiente, con plazos malos.
    let manana = (NOW / DAY_NS + 1) * DAY_NS;
    let mut z = afb(USDC_UNIT, 1);
    z.release_deadline = manana;
    afb_falla(&mut c, AGENT, manana, PAYER, &z, "ARRAS_E50");

    // E11 antes que E67, E67 antes que E12 y E12 antes que E13, con saldo.
    let mut c = con_presupuesto();
    let mut z = afb(USDC_UNIT, 1);
    z.release_deadline = NOW;
    z.fee_bps_max = 0;
    afb_falla(&mut c, AGENT, NOW, PAYER, &z, "ARRAS_E11");
    let mut z = afb(USDC_UNIT, 1);
    z.dispute_deadline = Some(RELEASE - 1);
    z.fee_bps_max = 0;
    afb_falla(&mut c, AGENT, NOW, PAYER, &z, "ARRAS_E67");
    ctx(AGENT, 0, NOW);
    call_afb(&mut c, PAYER, &afb(USDC_UNIT, 1));
    c.operators.get_mut(OP).unwrap().fee_bps = 1301;
    afb_falla(&mut c, AGENT, NOW, PAYER, &afb(USDC_UNIT, 1), "ARRAS_E12");
    c.operators.get_mut(OP).unwrap().fee_bps = 1300;
    // E13 antes que E53: el operador con llave y el pago repetido sin ticket.
    con_ticket(&mut c);
    afb_falla(&mut c, AGENT, NOW, PAYER, &afb(USDC_UNIT, 1), "ARRAS_E13");
    // E53 antes que E14: el owner sin storage y sin ticket.
    let disp = c.storage.get(&acc(PAYER)).unwrap().available;
    c.storage.get_mut(&acc(PAYER)).unwrap().available = 0;
    c.storage_available_total -= disp;
    afb_falla(&mut c, AGENT, NOW, PAYER, &afb(USDC_UNIT, 2), "ARRAS_E53");
    // Con un ticket valido, lo que queda es E14.
    let mut t = afb(USDC_UNIT, 2);
    let id = afb_id(PAYER, &t);
    t.ticket = Some(firmar_ticket(
        crate::ticket::TicketSubject::Payment { payment_id: &id },
        RELEASE,
    ));
    afb_falla(&mut c, AGENT, NOW, PAYER, &t, "ARRAS_E14");
}

#[test]
fn el_pago_del_agente_queda_en_escrow_y_vuelve_al_owner() {
    let mut c = con_presupuesto();
    ctx(AGENT, 0, NOW);
    let id1 = call_afb(&mut c, PAYER, &afb(USDC_UNIT, 1));
    let id2 = call_afb(&mut c, PAYER, &afb(USDC_UNIT, 2));
    let id3 = call_afb(&mut c, PAYER, &afb(USDC_UNIT, 3));
    // El agente no libera, no reembolsa y no reclama.
    ctx(AGENT, 0, NOW);
    expect_panic("ARRAS_E21", || c.release(id1.clone(), None));
    expect_panic("ARRAS_E25", || c.refund(id1.clone()));
    ctx(AGENT, 0, RECLAIM);
    expect_panic("ARRAS_E26", || c.reclaim(id1.clone()));
    // El capturer libera al worker (con la tarifa) y reembolsa al owner; el owner reclama el tercero.
    ctx(CAPTURER, 0, NOW);
    c.release(id1, None);
    c.refund(id2);
    ctx(PAYER, 0, RECLAIM);
    c.reclaim(id3);
    assert_eq!(c.claimable_of(&acc(WORKER)), 870_000);
    assert_eq!(c.claimable_of(&acc(FEES)), 130_000);
    assert_eq!(
        c.claimable_of(&acc(PAYER)),
        2 * USDC_UNIT,
        "refund y reclaim van al owner"
    );
    assert_eq!(c.claimable_of(&acc(AGENT)), 0, "nunca al agente");
    assert_eq!(
        c.budget_of(PAYER, AGENT, OP).unwrap().saldo.0,
        2 * USDC_UNIT,
        "ni al presupuesto"
    );
    assert_invariante(&c, &TODAS);
}

#[test]
fn withdraw_budget_solo_el_owner_y_en_pausa() {
    let mut c = con_presupuesto();
    // El agente o un tercero no tienen presupuesto propio: E50.
    for quien in [AGENT, OTHER] {
        ctx(quien, 0, NOW);
        let antes = snapshot(&mut c, PAYER);
        expect_panic("ARRAS_E50", || c.withdraw_budget(acc(AGENT), OP.into()));
        assert_eq!(snapshot(&mut c, PAYER), antes);
    }
    c.paused = true;
    ctx(PAYER, 0, NOW);
    c.withdraw_budget(acc(AGENT), OP.into());
    assert_eq!(
        logs(),
        vec![format!(
            r#"EVENT_JSON:{{"standard":"arras","version":"1.0.0","event":"budget_withdrawn","data":[{{"owner":"{PAYER}","agent":"{AGENT}","operator_id":"em","amount":"5000000"}}]}}"#
        )]
    );
    assert_eq!(c.claimable_of(&acc(PAYER)), 5 * USDC_UNIT);
    let b = c.budget_of(PAYER, AGENT, OP).unwrap();
    assert_eq!(
        (b.saldo.0, b.tope_por_pago.0),
        (0, TOPES.0),
        "queda en 0 con sus topes"
    );
    assert_eq!(
        c.tvl_global,
        5 * USDC_UNIT,
        "tvl_global baja recien al retirar"
    );
    assert_eq!(c.payer_tvl_of(OP, &acc(PAYER)), 0);
    assert_eq!(c.get_operator(OP.into()).unwrap().tvl_operador.0, 0);
    expect_panic("ARRAS_E30", || c.withdraw_budget(acc(AGENT), OP.into()));
    expect_panic("ARRAS_E50", || c.withdraw_budget(acc(AGENT), "kk".into()));
    assert_invariante(&c, &TODAS);
    // Y el retiro saca la plata del contrato.
    let _ = c.withdraw();
    assert_eq!(
        on_withdraw_con(&mut c, PAYER, 5 * USDC_UNIT, true, NOW),
        5 * USDC_UNIT
    );
    assert_eq!(c.tvl_global, 0);
    assert_invariante(&c, &TODAS);
}

#[test]
fn revoke_agent_devuelve_saldo_y_storage() {
    let mut c = con_presupuesto();
    ctx(AGENT, 0, NOW);
    let id = call_afb(&mut c, PAYER, &afb(USDC_UNIT, 1));
    c.paused = true;
    ctx(PAYER, 0, NOW);
    let charged = c.budget_of(PAYER, AGENT, OP).unwrap().storage_charged.0;
    let antes = snapshot(&mut c, PAYER);
    c.revoke_agent(acc(AGENT), OP.into());
    let despues = snapshot(&mut c, PAYER);
    assert_eq!(c.budget_of(PAYER, AGENT, OP), None);
    assert_eq!(c.claimable_of(&acc(PAYER)), 4 * USDC_UNIT);
    let liberado = u128::from(antes.storage_usage - despues.storage_usage)
        * env::storage_byte_cost().as_yoctonear();
    assert_eq!(
        liberado,
        u128::from(record_bytes(KEY_BYTES, crate::storage::BUDGET_VALUE_BYTES))
            * env::storage_byte_cost().as_yoctonear(),
        "se libera el registro del presupuesto"
    );
    assert!(liberado < charged, "payer_tvl y claimable[owner] se quedan");
    assert_eq!(
        despues.storage_payer.unwrap().1,
        antes.storage_payer.unwrap().1 + liberado
    );
    assert_eq!(
        logs(),
        vec![format!(
            r#"EVENT_JSON:{{"standard":"arras","version":"1.0.0","event":"agent_revoked","data":[{{"owner":"{PAYER}","agent":"{AGENT}","operator_id":"em","amount":"4000000"}}]}}"#
        )]
    );
    // El agente revocado no autoriza mas; el pago que ya autorizo sigue (es del owner).
    c.paused = false;
    afb_falla(&mut c, AGENT, NOW, PAYER, &afb(USDC_UNIT, 2), "ARRAS_E50");
    ctx(CAPTURER, 0, NOW);
    c.release(id, None);
    expect_panic("ARRAS_E50", || {
        ctx(PAYER, 0, NOW);
        c.revoke_agent(acc(AGENT), OP.into())
    });
    assert_invariante(&c, &TODAS);
    // Un presupuesto nuevo para el mismo agente vuelve a cobrar el registro (las otras entradas ya existen).
    let antes = snapshot(&mut c, PAYER);
    deposit(&mut c, PAYER, AGENT, USDC_UNIT, TOPES);
    let despues = snapshot(&mut c, PAYER);
    assert_eq!(
        despues.storage_usage - antes.storage_usage,
        record_bytes(KEY_BYTES, crate::storage::BUDGET_VALUE_BYTES)
    );
}

#[test]
fn revoke_agent_sin_saldo_y_patrocinado_devuelve_al_pool() {
    let mut c = setup();
    ctx(OWNER, near(1).as_yoctonear(), NOW);
    c.fund_operator_storage(OP.into());
    // OTHER no tiene NEP-145: paga el patrocinio.
    deposit(&mut c, OTHER, AGENT, USDC_UNIT, TOPES);
    let b = c.budget_of(OTHER, AGENT, OP).unwrap();
    assert!(b.sponsored);
    assert_eq!(c.sponsored_used_of(OP, &acc(OTHER)), b.storage_charged.0);
    let pool = c.get_operator(OP.into()).unwrap().sponsor_pool.0;
    assert_eq!(pool, near(1).as_yoctonear() - b.storage_charged.0);
    ctx(OTHER, 0, NOW);
    c.withdraw_budget(acc(AGENT), OP.into());
    let antes = snapshot(&mut c, OTHER);
    c.revoke_agent(acc(AGENT), OP.into());
    let despues = snapshot(&mut c, OTHER);
    let liberado = u128::from(antes.storage_usage - despues.storage_usage)
        * env::storage_byte_cost().as_yoctonear();
    assert_eq!(
        c.get_operator(OP.into()).unwrap().sponsor_pool.0,
        pool + liberado
    );
    assert_eq!(
        c.sponsored_used_of(OP, &acc(OTHER)),
        b.storage_charged.0 - liberado
    );
    assert_eq!(
        c.claimable_of(&acc(OTHER)),
        USDC_UNIT,
        "el saldo ya estaba en claimable"
    );
    assert_invariante(&c, &[PAYER, WORKER, FEES, OTHER]);
}

#[test]
fn set_budget_caps_con_un_yocto_y_solo_el_owner() {
    let mut c = con_presupuesto();
    ctx(PAYER, 0, NOW);
    expect_panic("Requires attached deposit of exactly 1 yoctoNEAR", || {
        c.set_budget_caps(acc(AGENT), OP.into(), U128(9), U128(9))
    });
    for quien in [AGENT, OTHER] {
        ctx(quien, 1, NOW);
        expect_panic("ARRAS_E50", || {
            c.set_budget_caps(acc(AGENT), OP.into(), U128(9), U128(9))
        });
    }
    ctx(PAYER, 1, NOW);
    for (pp, pd) in [(0u128, 1u128), (1, 0)] {
        expect_panic("ARRAS_E56", || {
            c.set_budget_caps(acc(AGENT), OP.into(), U128(pp), U128(pd))
        });
    }
    c.set_budget_caps(
        acc(AGENT),
        OP.into(),
        U128(4 * USDC_UNIT),
        U128(5 * USDC_UNIT),
    );
    assert_eq!(
        logs(),
        vec![format!(
            r#"EVENT_JSON:{{"standard":"arras","version":"1.0.0","event":"budget_caps_set","data":[{{"owner":"{PAYER}","agent":"{AGENT}","operator_id":"em","tope_por_pago":"4000000","tope_diario":"5000000"}}]}}"#
        )]
    );
    // Ahora entra un pago de 4 (antes E52).
    ctx(AGENT, 0, NOW);
    call_afb(&mut c, PAYER, &afb(4 * USDC_UNIT, 1));
    assert_invariante(&c, &TODAS);
}

#[test]
fn get_budget_muestra_lo_disponible_hoy() {
    let mut c = con_presupuesto();
    assert_eq!(c.get_budget(acc(PAYER), acc(OTHER), OP.into()), None);
    let v = c.get_budget(acc(PAYER), acc(AGENT), OP.into()).unwrap();
    assert_eq!(
        (v.saldo.0, v.disponible_hoy.0, v.gastado_hoy.0),
        (5 * USDC_UNIT, TOPES.1, 0)
    );
    assert_eq!(v.token, acc(USDC));
    ctx(AGENT, 0, NOW);
    call_afb(&mut c, PAYER, &afb(TOPES.0, 1));
    let v = c.get_budget(acc(PAYER), acc(AGENT), OP.into()).unwrap();
    assert_eq!(
        (v.saldo.0, v.disponible_hoy.0, v.gastado_hoy.0),
        (3 * USDC_UNIT, USDC_UNIT, TOPES.0)
    );
    // Otro dia: lo gastado vuelve a 0 en la vista; el saldo limita.
    ctx(OTHER, 0, NOW + DAY_NS);
    let v = c.get_budget(acc(PAYER), acc(AGENT), OP.into()).unwrap();
    assert_eq!((v.disponible_hoy.0, v.gastado_hoy.0), (TOPES.1, 0));
    c.budgets
        .get_mut(&(acc(PAYER), acc(AGENT), OP.to_string()))
        .unwrap()
        .saldo = U128(1);
    let v = c.get_budget(acc(PAYER), acc(AGENT), OP.into()).unwrap();
    assert_eq!(v.disponible_hoy.0, 1);
}

#[test]
fn budget_value_bytes_es_el_borsh_real() {
    let b = Budget {
        saldo: U128(u128::MAX),
        tope_por_pago: U128(1),
        tope_diario: U128(1),
        gastado_hoy: U128(0),
        dia: U64(0),
        storage_charged: U128(0),
        sponsored: true,
    };
    assert_eq!(
        near_sdk::borsh::to_vec(&b).unwrap().len() as u64,
        crate::storage::BUDGET_VALUE_BYTES
    );
}
