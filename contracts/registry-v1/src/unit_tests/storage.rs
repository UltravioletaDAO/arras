//! U-ST: el storage que se cobra. `FEEDBACK_MAX_BYTES` y `AGENT_MAX_BYTES` son exactamente el peor caso medido.

use super::common::*;
use crate::*;
use near_sdk::serde_json::json;

fn al_tope(escrow: &str) -> Pedido {
    Pedido {
        escrow: escrow.into(),
        payment_id: "p".repeat(MAX_PAYMENT_ID_LEN),
        score: 100,
        tag1: Some("t".repeat(MAX_TAG_LEN)),
        tag2: Some("u".repeat(MAX_TAG_LEN)),
        feedback_uri: Some(format!("https://{}", "a".repeat(MAX_FEEDBACK_URI_LEN - 8))),
        feedback_hash: Some(HASH.into()),
    }
}

fn pago_al_tope(rater: &str, ratee: &str) -> String {
    let mut pago = payment_v1(rater, ratee, "Released", u128::MAX);
    pago["operator_id"] = json!("o".repeat(MAX_OPERATOR_ID_LEN));
    pago.to_string()
}

/// give + callback; devuelve los bytes que el callback agrego (sin el candado que borra).
fn medir(c: &mut Registry, rater: &str, p: &Pedido, pago: &str) -> u64 {
    let args = give(c, rater, p);
    ctx_callback(ok_bytes(pago), HEIGHT + 2);
    c.flush_all();
    let antes = env::storage_usage();
    assert!(c.on_payment_checked(args).is_some(), "{:?}", logs());
    c.flush_all();
    env::storage_usage() + PENDING_RECORD_BYTES - antes
}

#[test]
fn ust1_peor_caso_de_una_calificacion_es_feedback_max_bytes() {
    // Cuentas de 64 caracteres (implicitas) para el escrow, el rater y la calificada.
    let escrow = "d".repeat(64);
    let rater = "b".repeat(64);
    let ratee = "c".repeat(64);
    let mut c = new_with(&[&escrow]);
    let bytes = medir(
        &mut c,
        &rater,
        &al_tope(&escrow),
        &pago_al_tope(&rater, &ratee),
    );
    assert_eq!(bytes, FEEDBACK_MAX_BYTES);
    assert_eq!(FEEDBACK_MAX_BYTES, 1643);
    // Se cobra todo el deposito: no vuelve nada.
    assert!(transfers().is_empty(), "{:?}", transfers());
    assert_eq!(deposit(), 1643 * 10u128.pow(19));
    // La segunda a la misma calificada no paga el indice ni el resumen nuevos.
    let p2 = Pedido {
        payment_id: "q".repeat(MAX_PAYMENT_ID_LEN),
        ..al_tope(&escrow)
    };
    let bytes2 = medir(&mut c, &rater, &p2, &pago_al_tope(&rater, &ratee));
    let indice_y_resumen = record_bytes(1 + 68, 4 + 4 + 70) + record_bytes(1 + 68, 44);
    assert_eq!(bytes2, FEEDBACK_MAX_BYTES - indice_y_resumen);
    assert_eq!(
        transfers(),
        vec![(rater.clone(), u128::from(indice_y_resumen) * 10u128.pow(19))]
    );
}

#[test]
fn ust2_una_calificacion_minima_cobra_lo_suyo() {
    let mut c = new_contract();
    let p = Pedido {
        tag1: None,
        tag2: None,
        feedback_uri: None,
        feedback_hash: None,
        ..Default::default()
    };
    let bytes = medir(&mut c, PAYER, &p, &released_json());
    assert!(bytes < FEEDBACK_MAX_BYTES);
    assert_eq!(
        transfers(),
        vec![(
            PAYER.to_string(),
            deposit() - u128::from(bytes) * 10u128.pow(19)
        )]
    );
}

#[test]
fn ust3_peor_caso_de_register_es_agent_max_bytes() {
    let who = "c".repeat(64);
    let mut c = new_contract();
    ctx(&who, 10u128.pow(24));
    c.flush_all();
    let antes = env::storage_usage();
    c.register(
        format!("https://{}", "a".repeat(MAX_AGENT_URI_LEN - 8)),
        Some("m".repeat(MAX_METADATA_LEN)),
    );
    c.flush_all();
    assert_eq!(env::storage_usage() - antes, AGENT_MAX_BYTES);
    assert_eq!(AGENT_MAX_BYTES, 1751);
    assert_eq!(c.get_config().register_deposit, U128(1751 * 10u128.pow(19)));
}

#[test]
fn ust4_un_rechazo_no_deja_storage() {
    // El mock de near-sdk vuelve `storage_usage` a su valor inicial en cada `testing_env!`: se mide dentro de cada
    // contexto.
    let mut c = new_contract();
    ctx(PAYER, deposit());
    c.flush_all();
    let antes = env::storage_usage();
    call_give(&mut c, &Pedido::default());
    c.flush_all();
    assert_eq!(
        env::storage_usage(),
        antes + PENDING_RECORD_BYTES,
        "el candado"
    );
    let args = callback_args();
    ctx_callback(ok_bytes("null"), HEIGHT + 2);
    c.flush_all();
    let antes = env::storage_usage();
    assert_eq!(c.on_payment_checked(args), None);
    c.flush_all();
    assert_eq!(
        env::storage_usage() + PENDING_RECORD_BYTES,
        antes,
        "el candado se borra"
    );
}
