//! `operator_ticket` en el contrato, y la administracion de los campos nuevos del operador (`arbiter`,
//! `arbiter_window_ns`, `ticket_pubkey`).

use super::common::*;
use crate::ticket::TicketSubject;
use crate::*;

/// El ticket del pago de `msg` (V-1 con otro salt) para PAYER.
fn ticket_de(
    msg: &arras_escrow_core::AuthorizeMsg,
    payer: &str,
    amount: u128,
    expiry: u64,
) -> TicketArg {
    let id = id_of(payer, amount, &msg.to_canonical_json());
    firmar_ticket(TicketSubject::Payment { payment_id: &id }, expiry)
}

fn m(n: u8) -> arras_escrow_core::AuthorizeMsg {
    let mut m = msg_v1();
    m.salt[15] = n;
    m
}

#[test]
fn sin_llave_no_hace_falta_y_un_ticket_de_mas_es_e53() {
    let mut c = setup();
    let t = ticket_de(&m(1), PAYER, USDC_UNIT, RELEASE);
    assert_rechazo(
        &mut c,
        USDC,
        PAYER,
        USDC_UNIT,
        &msg_v2(m(1), None, Some(&t)),
        "E53_TICKET_INVALIDO",
    );
    authorize(&mut c, PAYER, USDC_UNIT, &m(1).to_canonical_json());
}

#[test]
fn con_llave_authorize_exige_ticket_valido_y_vigente() {
    let mut c = setup();
    con_ticket(&mut c);
    // Sin ticket.
    assert_rechazo(
        &mut c,
        USDC,
        PAYER,
        USDC_UNIT,
        &m(1).to_canonical_json(),
        "E53_TICKET_INVALIDO",
    );
    // Vencido: now == expiry ya no vale (E54), aunque la firma sea buena.
    let t = ticket_de(&m(1), PAYER, USDC_UNIT, NOW);
    assert_rechazo(
        &mut c,
        USDC,
        PAYER,
        USDC_UNIT,
        &msg_v2(m(1), None, Some(&t)),
        "E54_TICKET_VENCIDO",
    );
    // De otro pago (otro salt, otro monto, otro payer), de otro contrato, mal formado o con expiry cambiado.
    let otros = [
        ticket_de(&m(2), PAYER, USDC_UNIT, RELEASE),
        ticket_de(&m(1), PAYER, USDC_UNIT + 1, RELEASE),
        ticket_de(&m(1), OTHER, USDC_UNIT, RELEASE),
        {
            use ed25519_dalek::Signer;
            let id = id_of(PAYER, USDC_UNIT, &m(1).to_canonical_json());
            let msg = crate::ticket::ticket_message(
                "v2b.arras-dev.testnet",
                TicketSubject::Payment { payment_id: &id },
                RELEASE,
            );
            TicketArg {
                expiry: U64(RELEASE),
                sig: near_sdk::bs58::encode(ticket_signing_key().sign(&msg).to_bytes())
                    .into_string(),
            }
        },
        TicketArg {
            expiry: U64(RELEASE),
            sig: "no-es-base58".into(),
        },
        TicketArg {
            expiry: U64(RELEASE + 1),
            sig: ticket_de(&m(1), PAYER, USDC_UNIT, RELEASE).sig,
        },
    ];
    for t in otros {
        assert_rechazo(
            &mut c,
            USDC,
            PAYER,
            USDC_UNIT,
            &msg_v2(m(1), None, Some(&t)),
            "E53_TICKET_INVALIDO",
        );
    }
    // Vigente hasta el ultimo nanosegundo.
    let t = ticket_de(&m(1), PAYER, USDC_UNIT, NOW + 1);
    let id = authorize(&mut c, PAYER, USDC_UNIT, &msg_v2(m(1), None, Some(&t)));
    assert_eq!(
        id,
        id_of(PAYER, USDC_UNIT, &m(1).to_canonical_json()),
        "el ticket no cambia el id"
    );
    // De un solo uso: el mismo pago otra vez es E13 (antes que el ticket).
    assert_rechazo(
        &mut c,
        USDC,
        PAYER,
        USDC_UNIT,
        &msg_v2(m(1), None, Some(&t)),
        "E13_ID_USADO",
    );
    assert_invariante(&c, &TODAS);
}

/// F-5: con la llave puesta, un atacante sin tickets no gasta el `sponsor_pool` ni ocupa TVL, y un payer con ticket
/// si usa el patrocinio. El ticket va antes que E14.
#[test]
fn con_llave_el_pool_no_se_vacia_sin_tickets() {
    let mut c = setup();
    ctx(OWNER, near(1).as_yoctonear(), NOW);
    c.fund_operator_storage(OP.into());
    con_ticket(&mut c);
    let pool = c.get_operator(OP.into()).unwrap().sponsor_pool.0;
    for n in 0..30u8 {
        let atacante = format!("atacante-{n}.testnet");
        assert_rechazo(
            &mut c,
            USDC,
            &atacante,
            MIN_AMOUNT,
            &m(n).to_canonical_json(),
            "E53_TICKET_INVALIDO",
        );
    }
    assert_eq!(c.get_operator(OP.into()).unwrap().sponsor_pool.0, pool);
    assert_eq!((c.tvl_global, c.payments.len()), (0, 0));
    let t = ticket_de(&m(1), OTHER, USDC_UNIT, RELEASE);
    let id = authorize(&mut c, OTHER, USDC_UNIT, &msg_v2(m(1), None, Some(&t)));
    assert!(c.get_payment(id).unwrap().sponsored);
    assert!(c.get_operator(OP.into()).unwrap().sponsor_pool.0 < pool);
}

#[test]
fn con_llave_el_deposito_exige_ticket_del_par_owner_agente() {
    let mut c = setup();
    con_ticket(&mut c);
    let sin = budget_msg(AGENT, USDC_UNIT, USDC_UNIT);
    assert_rechazo(
        &mut c,
        USDC,
        PAYER,
        USDC_UNIT,
        &sin.to_canonical_json(),
        "E53_TICKET_INVALIDO",
    );
    let firma = |op: &str, owner: &str, agent: &str, expiry: u64| {
        firmar_ticket(
            TicketSubject::Budget {
                operator_id: op,
                owner,
                agent,
            },
            expiry,
        )
    };
    let con = |t: TicketArg| {
        let mut b = budget_msg(AGENT, USDC_UNIT, USDC_UNIT);
        b.operator_ticket = Some(crate::msg::OperatorTicket {
            expiry: t.expiry.0,
            sig: t.sig,
        });
        b.to_canonical_json()
    };
    for t in [
        firma(OP, PAYER, "otro-agente.testnet", RELEASE),
        firma(OP, OTHER, AGENT, RELEASE),
        firma("kk", PAYER, AGENT, RELEASE),
        // Un ticket de pago no sirve para un deposito.
        firmar_ticket(TicketSubject::Payment { payment_id: AGENT }, RELEASE),
    ] {
        assert_rechazo(
            &mut c,
            USDC,
            PAYER,
            USDC_UNIT,
            &con(t),
            "E53_TICKET_INVALIDO",
        );
    }
    assert_rechazo(
        &mut c,
        USDC,
        PAYER,
        USDC_UNIT,
        &con(firma(OP, PAYER, AGENT, NOW)),
        "E54_TICKET_VENCIDO",
    );
    let ok = con(firma(OP, PAYER, AGENT, RELEASE));
    let back = ft_on_transfer_as(&mut c, USDC, PAYER, USDC_UNIT, &ok);
    assert_eq!(back, 0, "{:?}", logs());
    // El mismo ticket vale para re-depositar mientras no venza (aprueba el par, no un monto).
    let back = ft_on_transfer_as(&mut c, USDC, PAYER, USDC_UNIT, &ok);
    assert_eq!(back, 0, "{:?}", logs());
    assert_eq!(
        c.budget_of(PAYER, AGENT, OP).unwrap().saldo.0,
        2 * USDC_UNIT
    );
    assert_invariante(&c, &TODAS);
}

#[test]
fn con_llave_authorize_from_budget_exige_ticket() {
    let mut c = setup();
    deposit(
        &mut c,
        PAYER,
        AGENT,
        3 * USDC_UNIT,
        (2 * USDC_UNIT, 3 * USDC_UNIT),
    );
    con_ticket(&mut c);
    let a = afb(USDC_UNIT, 1);
    ctx(AGENT, 0, NOW);
    expect_panic("ARRAS_E53", || call_afb(&mut c, PAYER, &a));
    let id = afb_id(PAYER, &a);
    let mut con = a.clone();
    con.ticket = Some(firmar_ticket(
        TicketSubject::Payment { payment_id: &id },
        NOW,
    ));
    expect_panic("ARRAS_E54", || call_afb(&mut c, PAYER, &con));
    con.ticket = Some(firmar_ticket(
        TicketSubject::Payment { payment_id: &id },
        RELEASE,
    ));
    // El ticket de este pago no sirve para otro.
    let mut otro = afb(USDC_UNIT, 2);
    otro.ticket = con.ticket.clone();
    expect_panic("ARRAS_E53", || call_afb(&mut c, PAYER, &otro));
    assert_eq!(call_afb(&mut c, PAYER, &con), id);
    assert_invariante(&c, &TODAS);
}

#[test]
fn quitar_y_rotar_la_llave() {
    let mut c = setup();
    con_ticket(&mut c);
    assert_eq!(
        c.get_operator(OP.into()).unwrap().ticket_pubkey,
        Some(ticket_pubkey())
    );
    // Poner y quitar a la vez: E48.
    ctx(OWNER, 1, NOW);
    expect_panic("ARRAS_E48", || {
        c.update_operator(
            OP.into(),
            OperatorPatch {
                ticket_pubkey: Some(ticket_pubkey()),
                quitar_ticket_pubkey: Some(true),
                ..Default::default()
            },
        )
    });
    // Rotar a otra llave: el ticket de la vieja ya no vale.
    let otra = PublicKey::from_parts(near_sdk::CurveType::ED25519, vec![7; 32]).unwrap();
    c.update_operator(
        OP.into(),
        OperatorPatch {
            ticket_pubkey: Some(otra),
            ..Default::default()
        },
    );
    let t = ticket_de(&m(1), PAYER, USDC_UNIT, RELEASE);
    assert_rechazo(
        &mut c,
        USDC,
        PAYER,
        USDC_UNIT,
        &msg_v2(m(1), None, Some(&t)),
        "E53_TICKET_INVALIDO",
    );
    // Quitarla: vuelve a no hacer falta (y `quitar_ticket_pubkey: false` no quita nada).
    ctx(OWNER, 1, NOW);
    c.update_operator(
        OP.into(),
        OperatorPatch {
            quitar_ticket_pubkey: Some(false),
            ..Default::default()
        },
    );
    assert!(c.get_operator(OP.into()).unwrap().ticket_pubkey.is_some());
    ctx(OWNER, 1, NOW);
    c.update_operator(
        OP.into(),
        OperatorPatch {
            quitar_ticket_pubkey: Some(true),
            ..Default::default()
        },
    );
    assert_eq!(
        logs(),
        vec![r#"EVENT_JSON:{"standard":"arras","version":"1.0.0","event":"operator_updated","data":[{"operator_id":"em","quitar_ticket_pubkey":true}]}"#.to_string()]
    );
    assert_eq!(c.get_operator(OP.into()).unwrap().ticket_pubkey, None);
    authorize(&mut c, PAYER, USDC_UNIT, &m(1).to_canonical_json());
}

#[test]
fn solo_llaves_ed25519_y_plazo_del_arbitro_en_rango() {
    let mut c = new_contract();
    let secp = PublicKey::from_parts(near_sdk::CurveType::SECP256K1, vec![1; 64]).unwrap();
    ctx(OWNER, 1, NOW);
    let mut i = operator_input();
    i.ticket_pubkey = Some(secp.clone());
    expect_panic("ARRAS_E48", || c.register_operator(OP.into(), i.clone()));
    for w in [
        0,
        MIN_ARBITER_WINDOW_NS - 1,
        MAX_ARBITER_WINDOW_NS + 1,
        u64::MAX,
    ] {
        let mut i = operator_input();
        i.arbiter_window_ns = U64(w);
        expect_panic("ARRAS_E47", || c.register_operator(OP.into(), i.clone()));
    }
    // E47 antes que E48 (y despues de E46).
    let mut i = operator_input();
    i.arbiter_window_ns = U64(0);
    i.ticket_pubkey = Some(secp.clone());
    expect_panic("ARRAS_E47", || c.register_operator(OP.into(), i.clone()));
    i.fee_recipient = acc(CONTRACT);
    expect_panic("ARRAS_E46", || c.register_operator(OP.into(), i));
    for w in [MIN_ARBITER_WINDOW_NS, MAX_ARBITER_WINDOW_NS] {
        let mut i = operator_input();
        i.arbiter_window_ns = U64(w);
        let id = format!("op{}", w % 7);
        c.register_operator(id.clone(), i);
        assert_eq!(c.get_operator(id).unwrap().arbiter_window_ns, U64(w));
    }
    let mut i = operator_input();
    i.ticket_pubkey = Some(ticket_pubkey());
    ctx(OWNER, 1, NOW);
    c.register_operator(OP.into(), i);
    let l = logs();
    assert_eq!(l.len(), 1);
    assert!(
        l[0].ends_with(&format!(
            r#""arbiter":"{ARBITER}","arbiter_window_ns":"{ARBITER_WINDOW}","ticket_pubkey":"{}"}}]}}"#,
            String::from(&ticket_pubkey())
        )),
        "{}",
        l[0]
    );
    // update_operator: las mismas guardas.
    ctx(OWNER, 1, NOW);
    expect_panic("ARRAS_E48", || {
        c.update_operator(
            OP.into(),
            OperatorPatch {
                ticket_pubkey: Some(secp),
                ..Default::default()
            },
        )
    });
    expect_panic("ARRAS_E47", || {
        c.update_operator(
            OP.into(),
            OperatorPatch {
                arbiter_window_ns: Some(U64(MAX_ARBITER_WINDOW_NS + 1)),
                ..Default::default()
            },
        )
    });
    // Y solo el owner, con 1 yocto.
    ctx(ARBITER, 1, NOW);
    expect_panic("ARRAS_E40", || {
        c.update_operator(
            OP.into(),
            OperatorPatch {
                arbiter: Some(acc(ARBITER)),
                ..Default::default()
            },
        )
    });
}
