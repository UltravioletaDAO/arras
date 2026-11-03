//! Reputacion: las guardas de `give_feedback` (E10..E19), las del callback (E18, E20..E26), la calificacion de cada
//! lado, el candado y `revoke_feedback` (E30, E31).

use super::common::*;
use crate::*;
use near_sdk::PromiseResult;
use near_sdk::serde_json::json;

// ---------------------------------------------------------------- la promesa

#[test]
fn give_feedback_llama_get_payment_y_el_callback_con_su_gas() {
    let mut c = new_contract();
    let p = Pedido::default();
    ctx(PAYER, deposit());
    call_give(&mut c, &p);
    let calls = function_calls();
    assert_eq!(calls.len(), 2, "{calls:?}");
    assert_eq!(calls[0].0, ESCROW);
    assert_eq!(calls[0].1, "get_payment");
    assert_eq!(calls[0].2, json!({ "payment_id": PID }));
    assert_eq!(calls[0].3, GAS_GET_PAYMENT);
    assert_eq!(calls[0].4, 0);
    assert_eq!(calls[1].0, CONTRACT);
    assert_eq!(calls[1].1, "on_payment_checked");
    assert_eq!(calls[1].3, GAS_ON_PAYMENT_CHECKED);
    assert_eq!(calls[1].4, 0);
    assert_eq!(
        callback_args(),
        PendingFeedback {
            rater: acc(PAYER),
            escrow: acc(ESCROW),
            payment_id: PID.into(),
            score: 90,
            tag1: Some("quality".into()),
            tag2: Some("successRate".into()),
            feedback_uri: Some("https://em.example/feedback/1.json".into()),
            feedback_hash: Some(HASH.into()),
            deposit: U128(deposit()),
            nonce: U64(HEIGHT),
        }
    );
    assert!(
        transfers().is_empty(),
        "el deposito queda hasta el callback"
    );
    assert!(c.is_feedback_pending(acc(ESCROW), PID.into(), acc(PAYER)));
    assert!(!c.is_feedback_pending(acc(ESCROW), PID.into(), acc(WORKER)));
}

#[test]
fn el_deposito_de_mas_viaja_entero_al_callback() {
    let mut c = new_contract();
    let args = give_at(&mut c, PAYER, deposit() * 3, HEIGHT, &Pedido::default());
    assert_eq!(args.deposit, U128(deposit() * 3));
}

// ---------------------------------------------------------------- aceptadas

#[test]
fn payer_califica_al_receiver_de_un_pago_released() {
    let mut c = new_contract();
    let p = Pedido::default();
    let args = give(&mut c, PAYER, &p);
    ctx_callback(ok_bytes(&released_json()), HEIGHT + 2);
    c.flush_all();
    let antes = env::storage_usage();
    let id = c.on_payment_checked(args);
    c.flush_all();
    // Lo que se cobra es la calificacion; el candado que el callback borra no cuenta.
    let bytes = env::storage_usage() + PENDING_RECORD_BYTES - antes;
    assert_eq!(id, Some(0));
    let costo = u128::from(bytes) * 10u128.pow(19);
    assert_eq!(transfers(), vec![(PAYER.to_string(), deposit() - costo)]);
    let f = c.get_feedback(0).unwrap();
    assert_eq!(
        f,
        FeedbackView {
            feedback_id: 0,
            rater: acc(PAYER),
            ratee: acc(WORKER),
            ratee_role: Role::Receiver,
            escrow: acc(ESCROW),
            payment_id: PID.into(),
            operator_id: "em".into(),
            released: U128(1_000_000),
            score: 90,
            tag1: Some("quality".into()),
            tag2: Some("successRate".into()),
            feedback_uri: Some("https://em.example/feedback/1.json".into()),
            feedback_hash: Some(HASH.into()),
            created_at: U64(NOW + 1_000_000_000),
            revoked: false,
            revoked_at: None,
        }
    );
    let ev = events("feedback_given");
    assert_eq!(ev.len(), 1);
    assert_eq!(
        ev[0]["data"],
        json!([{
            "feedback_id": 0,
            "rater": PAYER,
            "ratee": WORKER,
            "ratee_role": "receiver",
            "escrow": ESCROW,
            "payment_id": PID,
            "operator_id": "em",
            "released": "1000000",
            "score": 90,
            "tag1": "quality",
            "tag2": "successRate",
            "feedback_uri": "https://em.example/feedback/1.json",
            "feedback_hash": HASH,
            "created_at": (NOW + 1_000_000_000).to_string(),
        }])
    );
    assert!(!c.is_feedback_pending(acc(ESCROW), PID.into(), acc(PAYER)));
    assert_eq!(
        c.get_feedback_for_payment(acc(ESCROW), PID.into(), acc(PAYER)),
        Some(f)
    );
    let s = c.get_summary(acc(WORKER));
    assert_eq!(s.count, 1);
    assert_eq!(s.as_receiver.count, 1);
    assert_eq!(s.as_payer.count, 0);
    assert_eq!(s.average_x100, Some(9000));
    assert_eq!(s.last_feedback_id, Some(0));
    assert_eq!(c.get_summary(acc(PAYER)).count, 0, "el rater no suma");
    assert_eq!(c.get_config().feedbacks, 1);
}

#[test]
fn receiver_califica_al_payer() {
    let mut c = new_contract();
    let p = Pedido {
        score: 40,
        tag1: None,
        tag2: None,
        feedback_uri: None,
        feedback_hash: None,
        ..Default::default()
    };
    assert_eq!(rate(&mut c, WORKER, &p, &released_json()), Some(0));
    let f = c.get_feedback(0).unwrap();
    assert_eq!(f.rater, acc(WORKER));
    assert_eq!(f.ratee, acc(PAYER));
    assert_eq!(f.ratee_role, Role::Payer);
    assert_eq!(f.score, 40);
    assert_eq!(f.tag1, None);
    assert_eq!(f.feedback_hash, None);
    let s = c.get_summary(acc(PAYER));
    assert_eq!(s.as_payer.count, 1);
    assert_eq!(s.as_payer.average_x100, Some(4000));
    assert_eq!(s.as_receiver.count, 0);
    assert_eq!(s.as_receiver.average_x100, None);
    assert_eq!(
        events("feedback_given")[0]["data"][0]["ratee_role"],
        "payer"
    );
}

#[test]
fn los_dos_lados_del_mismo_pago() {
    let mut c = new_contract();
    let p = Pedido::default();
    assert_eq!(rate(&mut c, PAYER, &p, &released_json()), Some(0));
    assert_eq!(rate(&mut c, WORKER, &p, &released_json()), Some(1));
    assert_eq!(c.get_feedback(0).unwrap().ratee, acc(WORKER));
    assert_eq!(c.get_feedback(1).unwrap().ratee, acc(PAYER));
}

#[test]
fn partially_released_con_plata_liberada_vale() {
    let mut c = new_contract();
    let pago = payment_v1(PAYER, WORKER, "PartiallyReleased", 1).to_string();
    assert_eq!(rate(&mut c, PAYER, &Pedido::default(), &pago), Some(0));
    assert_eq!(c.get_feedback(0).unwrap().released, U128(1));
}

#[test]
fn score_0_y_100_valen() {
    let mut c = new_contract();
    let p0 = Pedido {
        score: 0,
        ..Default::default()
    };
    assert_eq!(rate(&mut c, PAYER, &p0, &released_json()), Some(0));
    let p100 = Pedido {
        score: 100,
        payment_id: PID2.into(),
        ..Default::default()
    };
    assert_eq!(rate(&mut c, PAYER, &p100, &released_json()), Some(1));
    let s = c.get_summary(acc(WORKER));
    assert_eq!(s.score_sum, U64(100));
    assert_eq!(s.average_x100, Some(5000));
}

#[test]
fn un_payment_de_v2_con_campos_de_mas_vale() {
    let mut c = new_contract();
    let mut pago = payment_v1(PAYER, WORKER, "Released", 500);
    pago["dispute_deadline"] = json!("1793577600000000000");
    pago["arbiter_window_ns"] = json!("86400000000000");
    pago["dispute"] = json!(null);
    let p = Pedido {
        escrow: ESCROW2.into(),
        ..Default::default()
    };
    assert_eq!(rate(&mut c, PAYER, &p, &pago.to_string()), Some(0));
    assert_eq!(c.get_feedback(0).unwrap().escrow, acc(ESCROW2));
}

#[test]
fn el_mismo_pago_en_otro_escrow_es_otra_calificacion() {
    let mut c = new_contract();
    assert_eq!(
        rate(&mut c, PAYER, &Pedido::default(), &released_json()),
        Some(0)
    );
    let p2 = Pedido {
        escrow: ESCROW2.into(),
        ..Default::default()
    };
    assert_eq!(rate(&mut c, PAYER, &p2, &released_json()), Some(1));
}

#[test]
fn operator_id_de_64_vale() {
    let mut c = new_contract();
    let mut pago = payment_v1(PAYER, WORKER, "Released", 1);
    pago["operator_id"] = json!("o".repeat(64));
    assert_eq!(
        rate(&mut c, PAYER, &Pedido::default(), &pago.to_string()),
        Some(0)
    );
}

// ---------------------------------------------------------------- rechazos del callback

fn rechazo_con(rater: &str, r: PromiseResult, e: RegistryError) {
    let mut c = new_contract();
    let p = Pedido::default();
    let args = give(&mut c, rater, &p);
    let out = callback(&mut c, args, r);
    assert_rechazo(&mut c, out, rater, &p, e);
    assert_eq!(c.get_config().feedbacks, 0);
    assert_eq!(c.get_summary(acc(WORKER)).count, 0);
    assert_eq!(c.get_summary(acc(PAYER)).count, 0);
}

fn rechazo_pago(rater: &str, pago: &str, e: RegistryError) {
    rechazo_con(rater, ok_bytes(pago), e);
}

#[test]
fn get_payment_que_falla_es_e20() {
    rechazo_con(
        PAYER,
        PromiseResult::Failed,
        RegistryError::E20GetPaymentFallo,
    );
}

#[test]
fn respuesta_de_mas_de_8_kib_es_e21() {
    let mut pago = payment_v1(PAYER, WORKER, "Released", 1);
    pago["relleno"] = json!("x".repeat(9 * 1024));
    rechazo_pago(
        PAYER,
        &pago.to_string(),
        RegistryError::E21RespuestaInvalida,
    );
}

#[test]
fn respuesta_de_8_kib_justos_se_lee() {
    let mut pago = payment_v1(PAYER, WORKER, "Released", 1);
    let base = pago.to_string().len();
    pago["relleno"] = json!("x".repeat(MAX_PAYMENT_RESULT_LEN - base - ",\"relleno\":\"\"".len()));
    let s = pago.to_string();
    assert_eq!(s.len(), MAX_PAYMENT_RESULT_LEN);
    let mut c = new_contract();
    assert_eq!(rate(&mut c, PAYER, &Pedido::default(), &s), Some(0));
}

#[test]
fn respuesta_que_no_deserializa_es_e21() {
    for basura in [
        "",
        "no es json",
        "\x00\x01",
        "{}",
        "[]",
        "42",
        r#"{"payer":"agente.testnet"}"#,
        r#"{"operator_id":"em","payer":"NO VALIDA","receiver":"worker.testnet","released":"1","status":"Released"}"#,
        r#"{"operator_id":"em","payer":"agente.testnet","receiver":"worker.testnet","released":1,"status":"Released"}"#,
        r#"{"operator_id":"em","payer":"agente.testnet","receiver":"worker.testnet","released":"1","status":{"Released":{}}}"#,
    ] {
        rechazo_pago(PAYER, basura, RegistryError::E21RespuestaInvalida);
    }
}

#[test]
fn operator_id_de_65_es_e21() {
    let mut pago = payment_v1(PAYER, WORKER, "Released", 1);
    pago["operator_id"] = json!("o".repeat(65));
    rechazo_pago(
        PAYER,
        &pago.to_string(),
        RegistryError::E21RespuestaInvalida,
    );
}

#[test]
fn pago_inexistente_es_e22() {
    rechazo_pago(PAYER, "null", RegistryError::E22PagoInexistente);
}

#[test]
fn pago_sin_plata_liberada_es_e23() {
    for (status, released) in [
        ("Authorized", 0),
        ("Refunded", 0),
        ("Reclaimed", 0),
        // Punto 2 del diseno: solo Released o PartiallyReleased (un Refunded tras una liberacion parcial no vale).
        ("Refunded", 400_000),
        ("Reclaimed", 400_000),
        ("Authorized", 1),
        ("Released", 0),
        ("PartiallyReleased", 0),
        ("Disputed", 400_000),
        ("Resolved", 400_000),
        ("released", 1_000_000),
        ("RELEASED", 1_000_000),
        (" Released", 1_000_000),
        ("", 1_000_000),
    ] {
        let pago = payment_v1(PAYER, WORKER, status, released).to_string();
        rechazo_pago(PAYER, &pago, RegistryError::E23PagoSinLiberar);
        rechazo_pago(WORKER, &pago, RegistryError::E23PagoSinLiberar);
    }
}

#[test]
fn un_tercero_es_e24() {
    let mut c = new_contract();
    let p = Pedido::default();
    let args = give(&mut c, OTHER, &p);
    let out = callback(&mut c, args, ok_bytes(&released_json()));
    assert_rechazo(&mut c, out, OTHER, &p, RegistryError::E24NoEsParte);
}

#[test]
fn calificarse_a_si_mismo_es_e25() {
    // Arras v1 no deja payer == receiver (E15), pero el registro no lo da por sentado.
    let pago = payment_v1(PAYER, PAYER, "Released", 1_000_000).to_string();
    rechazo_pago(PAYER, &pago, RegistryError::E25AutoCalificacion);
}

#[test]
fn el_callback_vuelve_a_mirar_si_ya_califico_e18() {
    // Inalcanzable con el candado y el nonce (defensa en profundidad): se arma el estado a mano.
    let mut c = new_contract();
    let p = Pedido::default();
    assert_eq!(rate(&mut c, PAYER, &p, &released_json()), Some(0));
    let key = feedback_key(&acc(ESCROW), PID, &acc(PAYER));
    c.pending.insert(key, HEIGHT + 10);
    let args = PendingFeedback {
        rater: acc(PAYER),
        escrow: acc(ESCROW),
        payment_id: PID.into(),
        score: 10,
        tag1: None,
        tag2: None,
        feedback_uri: None,
        feedback_hash: None,
        deposit: U128(deposit()),
        nonce: U64(HEIGHT + 10),
    };
    let out = callback(&mut c, args, ok_bytes(&released_json()));
    assert_eq!(out, None);
    assert!(logs().contains(&RegistryError::E18YaCalificado.rejection_log()));
    assert_eq!(transfers(), vec![(PAYER.to_string(), deposit())]);
    assert_eq!(c.get_config().feedbacks, 1);
    assert!(c.pending.get(&key).is_none(), "el candado se libera");
}

#[test]
fn un_rechazo_deja_volver_a_intentar() {
    let mut c = new_contract();
    let p = Pedido::default();
    let args = give(&mut c, PAYER, &p);
    assert_eq!(callback(&mut c, args, PromiseResult::Failed), None);
    assert_eq!(rate(&mut c, PAYER, &p, &released_json()), Some(0));
}

// ---------------------------------------------------------------- candado

#[test]
#[should_panic(expected = "ARRAS_REG_E19")]
fn dos_give_feedback_en_vuelo_es_e19() {
    let mut c = new_contract();
    let p = Pedido::default();
    give(&mut c, PAYER, &p);
    give_at(
        &mut c,
        PAYER,
        deposit(),
        HEIGHT + PENDING_TTL_BLOCKS - 1,
        &p,
    );
}

#[test]
fn el_candado_es_por_rater_y_por_escrow() {
    let mut c = new_contract();
    let p = Pedido::default();
    give(&mut c, PAYER, &p);
    give(&mut c, WORKER, &p);
    give(
        &mut c,
        PAYER,
        &Pedido {
            escrow: ESCROW2.into(),
            ..Default::default()
        },
    );
    give(
        &mut c,
        PAYER,
        &Pedido {
            payment_id: PID2.into(),
            ..Default::default()
        },
    );
}

#[test]
fn candado_vencido_se_pisa_y_el_callback_viejo_es_e26() {
    let mut c = new_contract();
    let p = Pedido::default();
    let viejo = give(&mut c, PAYER, &p);
    ctx_at(PAYER, 0, HEIGHT + PENDING_TTL_BLOCKS);
    assert!(
        !c.is_feedback_pending(acc(ESCROW), PID.into(), acc(PAYER)),
        "vencido"
    );
    let nuevo = give_at(&mut c, PAYER, deposit(), HEIGHT + PENDING_TTL_BLOCKS, &p);
    assert_eq!(nuevo.nonce, U64(HEIGHT + PENDING_TTL_BLOCKS));
    // El callback viejo llega tarde: no es su candado.
    let out = callback(&mut c, viejo, ok_bytes(&released_json()));
    assert_eq!(out, None);
    assert!(logs().contains(&RegistryError::E26Reemplazado.rejection_log()));
    assert_eq!(transfers(), vec![(PAYER.to_string(), deposit())]);
    assert!(
        c.is_feedback_pending(acc(ESCROW), PID.into(), acc(PAYER)),
        "el candado del nuevo sigue"
    );
    // El nuevo califica.
    assert_eq!(callback(&mut c, nuevo, ok_bytes(&released_json())), Some(0));
    assert!(!c.is_feedback_pending(acc(ESCROW), PID.into(), acc(PAYER)));
}

#[test]
fn callback_sin_candado_es_e26() {
    let mut c = new_contract();
    let p = Pedido::default();
    let args = give(&mut c, PAYER, &p);
    let mut otro = args.clone();
    otro.nonce = U64(HEIGHT - 1);
    let out = callback(&mut c, otro, ok_bytes(&released_json()));
    assert_eq!(out, None);
    assert!(logs().contains(&RegistryError::E26Reemplazado.rejection_log()));
    assert_eq!(c.get_config().feedbacks, 0);
    assert!(c.is_feedback_pending(acc(ESCROW), PID.into(), acc(PAYER)));
    assert_eq!(callback(&mut c, args, ok_bytes(&released_json())), Some(0));
}

// ---------------------------------------------------------------- guardas de give_feedback (panic)

fn give_con(p: Pedido) {
    let mut c = new_contract();
    give(&mut c, PAYER, &p);
}

#[test]
#[should_panic(expected = "ARRAS_REG_E10")]
fn escrow_fuera_de_la_lista_es_e10() {
    give_con(Pedido {
        escrow: FUERA.into(),
        ..Default::default()
    });
}

#[test]
#[should_panic(expected = "ARRAS_REG_E10")]
fn el_propio_registro_como_escrow_es_e10() {
    give_con(Pedido {
        escrow: CONTRACT.into(),
        ..Default::default()
    });
}

#[test]
#[should_panic(expected = "ARRAS_REG_E11")]
fn payment_id_vacio_es_e11() {
    give_con(Pedido {
        payment_id: String::new(),
        ..Default::default()
    });
}

#[test]
#[should_panic(expected = "ARRAS_REG_E11")]
fn payment_id_largo_es_e11() {
    give_con(Pedido {
        payment_id: "p".repeat(65),
        ..Default::default()
    });
}

#[test]
#[should_panic(expected = "ARRAS_REG_E11")]
fn payment_id_con_espacio_es_e11() {
    give_con(Pedido {
        payment_id: format!("{PID} "),
        ..Default::default()
    });
}

#[test]
#[should_panic(expected = "ARRAS_REG_E12")]
fn score_101_es_e12() {
    give_con(Pedido {
        score: 101,
        ..Default::default()
    });
}

#[test]
#[should_panic(expected = "ARRAS_REG_E12")]
fn score_255_es_e12() {
    give_con(Pedido {
        score: 255,
        ..Default::default()
    });
}

#[test]
#[should_panic(expected = "ARRAS_REG_E13")]
fn tag1_invalido_es_e13() {
    give_con(Pedido {
        tag1: Some("con espacio".into()),
        ..Default::default()
    });
}

#[test]
#[should_panic(expected = "ARRAS_REG_E13")]
fn tag2_invalido_es_e13() {
    give_con(Pedido {
        tag2: Some(String::new()),
        ..Default::default()
    });
}

#[test]
#[should_panic(expected = "ARRAS_REG_E14")]
fn feedback_uri_invalida_es_e14() {
    give_con(Pedido {
        feedback_uri: Some("sin-esquema".into()),
        ..Default::default()
    });
}

#[test]
#[should_panic(expected = "ARRAS_REG_E15")]
fn feedback_hash_en_mayusculas_es_e15() {
    give_con(Pedido {
        feedback_hash: Some(HASH.to_uppercase()),
        ..Default::default()
    });
}

#[test]
#[should_panic(expected = "ARRAS_REG_E15")]
fn feedback_hash_con_0x_es_e15() {
    give_con(Pedido {
        feedback_hash: Some(format!("0x{}", &HASH[2..])),
        ..Default::default()
    });
}

#[test]
fn deposito_justo_pasa() {
    let mut c = new_contract();
    let args = give_at(&mut c, PAYER, deposit(), HEIGHT, &Pedido::default());
    assert_eq!(args.deposit, U128(deposit()));
}

#[test]
#[should_panic(
    expected = "ARRAS_REG_E16: deposito insuficiente para la calificacion: se necesitan"
)]
fn un_yocto_menos_de_deposito_es_e16() {
    let mut c = new_contract();
    give_at(&mut c, PAYER, deposit() - 1, HEIGHT, &Pedido::default());
}

#[test]
#[should_panic(expected = "ARRAS_REG_E16")]
fn sin_deposito_es_e16() {
    let mut c = new_contract();
    give_at(&mut c, PAYER, 0, HEIGHT, &Pedido::default());
}

#[test]
fn gas_minimo_justo_pasa() {
    let mut c = new_contract();
    ctx_full(PAYER, deposit(), NOW, HEIGHT, MIN_GAS_GIVE_FEEDBACK);
    call_give(&mut c, &Pedido::default());
    assert!(c.is_feedback_pending(acc(ESCROW), PID.into(), acc(PAYER)));
}

#[test]
#[should_panic(expected = "ARRAS_REG_E17")]
fn gas_de_menos_es_e17() {
    let mut c = new_contract();
    ctx_full(
        PAYER,
        deposit(),
        NOW,
        HEIGHT,
        Gas::from_gas(MIN_GAS_GIVE_FEEDBACK.as_gas() - 1),
    );
    call_give(&mut c, &Pedido::default());
}

#[test]
#[should_panic(expected = "ARRAS_REG_E18")]
fn la_segunda_calificacion_del_mismo_rater_y_pago_es_e18() {
    let mut c = new_contract();
    let p = Pedido::default();
    assert_eq!(rate(&mut c, PAYER, &p, &released_json()), Some(0));
    give_at(&mut c, PAYER, deposit(), HEIGHT + 5_000, &p);
}

#[test]
fn la_segunda_calificacion_no_toca_la_primera() {
    let mut c = new_contract();
    let p = Pedido::default();
    assert_eq!(rate(&mut c, PAYER, &p, &released_json()), Some(0));
    let otra = Pedido {
        score: 1,
        ..Default::default()
    };
    ctx(PAYER, deposit());
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| call_give(&mut c, &otra)));
    assert!(r.is_err());
    assert_eq!(c.get_feedback(0).unwrap().score, 90);
    assert_eq!(c.get_config().feedbacks, 1);
}

// ---------------------------------------------------------------- deposito y tope del cobro

#[test]
fn el_cobro_no_pasa_del_deposito() {
    // Un deposito menor que lo medido (imposible por E16; se arma a mano): se cobra el deposito y no se devuelve nada,
    // sin desbordar.
    let mut c = new_contract();
    let mut args = give(&mut c, PAYER, &Pedido::default());
    args.deposit = U128(1);
    assert_eq!(callback(&mut c, args, ok_bytes(&released_json())), Some(0));
    assert!(transfers().is_empty(), "{:?}", transfers());
}

// ---------------------------------------------------------------- revocar

fn revoke(c: &mut Registry, who: &str, yocto: u128, escrow: &str, pid: &str) -> FeedbackView {
    ctx_full(who, yocto, NOW + 7, HEIGHT + 3, Gas::from_tgas(300));
    c.revoke_feedback(acc(escrow), pid.into())
}

#[test]
fn revocar_marca_y_descuenta_del_resumen() {
    let mut c = new_contract();
    let p = Pedido::default();
    assert_eq!(rate(&mut c, PAYER, &p, &released_json()), Some(0));
    let p2 = Pedido {
        payment_id: PID2.into(),
        score: 50,
        ..Default::default()
    };
    assert_eq!(rate(&mut c, PAYER, &p2, &released_json()), Some(1));
    ctx_full(PAYER, 1, NOW + 7, HEIGHT + 3, Gas::from_tgas(300));
    c.flush_all();
    let antes = env::storage_usage();
    let v = c.revoke_feedback(acc(ESCROW), PID.into());
    c.flush_all();
    assert_eq!(env::storage_usage(), antes, "revocar no cambia el storage");
    assert!(v.revoked);
    assert_eq!(v.revoked_at, Some(U64(NOW + 7)));
    assert_eq!(c.get_feedback(0).unwrap(), v, "queda guardada, marcada");
    let s = c.get_summary(acc(WORKER));
    assert_eq!(s.count, 1);
    assert_eq!(s.revoked, 1);
    assert_eq!(s.score_sum, U64(50));
    assert_eq!(s.average_x100, Some(5000));
    assert_eq!(s.as_receiver.revoked, 1);
    assert_eq!(s.last_feedback_id, Some(1));
    let ev = events("feedback_revoked");
    assert_eq!(
        ev[0]["data"],
        json!([{
            "feedback_id": 0,
            "rater": PAYER,
            "ratee": WORKER,
            "escrow": ESCROW,
            "payment_id": PID,
            "revoked_at": (NOW + 7).to_string(),
        }])
    );
    assert_eq!(c.list_feedback(acc(WORKER), None, None).len(), 2);
}

#[test]
fn revocar_la_del_payer_descuenta_del_rol_payer() {
    let mut c = new_contract();
    assert_eq!(
        rate(&mut c, WORKER, &Pedido::default(), &released_json()),
        Some(0)
    );
    revoke(&mut c, WORKER, 1, ESCROW, PID);
    let s = c.get_summary(acc(PAYER));
    assert_eq!(s.as_payer.count, 0);
    assert_eq!(s.as_payer.revoked, 1);
    assert_eq!(s.as_payer.score_sum, U64(0));
    assert_eq!(s.as_payer.average_x100, None);
    assert_eq!(s.average_x100, None);
}

#[test]
#[should_panic(expected = "ARRAS_REG_E31")]
fn revocar_dos_veces_es_e31() {
    let mut c = new_contract();
    assert_eq!(
        rate(&mut c, PAYER, &Pedido::default(), &released_json()),
        Some(0)
    );
    revoke(&mut c, PAYER, 1, ESCROW, PID);
    revoke(&mut c, PAYER, 1, ESCROW, PID);
}

#[test]
#[should_panic(expected = "ARRAS_REG_E30")]
fn revocar_una_inexistente_es_e30() {
    let mut c = new_contract();
    revoke(&mut c, PAYER, 1, ESCROW, PID);
}

#[test]
#[should_panic(expected = "ARRAS_REG_E30")]
fn la_calificada_no_puede_revocar_e30() {
    let mut c = new_contract();
    assert_eq!(
        rate(&mut c, PAYER, &Pedido::default(), &released_json()),
        Some(0)
    );
    revoke(&mut c, WORKER, 1, ESCROW, PID);
}

#[test]
#[should_panic(expected = "Requires attached deposit of exactly 1 yoctoNEAR")]
fn revocar_sin_yocto() {
    let mut c = new_contract();
    assert_eq!(
        rate(&mut c, PAYER, &Pedido::default(), &released_json()),
        Some(0)
    );
    revoke(&mut c, PAYER, 0, ESCROW, PID);
}

#[test]
fn revocar_y_no_se_puede_volver_a_calificar() {
    let mut c = new_contract();
    let p = Pedido::default();
    assert_eq!(rate(&mut c, PAYER, &p, &released_json()), Some(0));
    revoke(&mut c, PAYER, 1, ESCROW, PID);
    ctx(PAYER, deposit());
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| call_give(&mut c, &p)));
    let msg = r.unwrap_err();
    let msg = msg.downcast_ref::<String>().cloned().unwrap_or_default();
    assert!(msg.contains("ARRAS_REG_E18"), "{msg}");
}
