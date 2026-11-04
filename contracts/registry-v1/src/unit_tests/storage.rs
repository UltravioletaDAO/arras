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
    assert_eq!(FEEDBACK_MAX_BYTES, 1864);
    // Se cobra todo el deposito: no vuelve nada.
    assert!(transfers().is_empty(), "{:?}", transfers());
    assert_eq!(deposit(), 1864 * 10u128.pow(19));
    // La segunda a la misma calificada y por el mismo operador no paga el indice ni los dos resumenes nuevos.
    let p2 = Pedido {
        payment_id: "q".repeat(MAX_PAYMENT_ID_LEN),
        ..al_tope(&escrow)
    };
    let bytes2 = medir(&mut c, &rater, &p2, &pago_al_tope(&rater, &ratee));
    let resumen_op = record_bytes(1 + 68 + 4 + 64, 44);
    assert_eq!(resumen_op, 221);
    let indice_y_resumenes =
        record_bytes(1 + 68, 4 + 4 + 70) + record_bytes(1 + 68, 44) + resumen_op;
    assert_eq!(bytes2, FEEDBACK_MAX_BYTES - indice_y_resumenes);
    assert_eq!(
        transfers(),
        vec![(
            rater.clone(),
            u128::from(indice_y_resumenes) * 10u128.pow(19)
        )]
    );
    // La tercera, por otro operador (tambien de 64): paga solo su resumen por operador nuevo, 221 bytes.
    let p3 = Pedido {
        payment_id: "r".repeat(MAX_PAYMENT_ID_LEN),
        ..al_tope(&escrow)
    };
    let mut pago3: near_sdk::serde_json::Value =
        near_sdk::serde_json::from_str(&pago_al_tope(&rater, &ratee)).unwrap();
    pago3["operator_id"] = json!("x".repeat(MAX_OPERATOR_ID_LEN));
    let bytes3 = medir(&mut c, &rater, &p3, &pago3.to_string());
    assert_eq!(bytes3, bytes2 + resumen_op);
    assert_eq!(
        transfers(),
        vec![(
            rater.clone(),
            u128::from(FEEDBACK_MAX_BYTES - bytes3) * 10u128.pow(19)
        )]
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

#[test]
fn ust5_el_resumen_por_operador_se_cobra_la_primera_vez() {
    // Cuentas y operador reales (`worker.testnet`, 14; `em`, 2). Lo paga el rater y vuelve el sobrante.
    let mut c = new_contract();
    let base = Pedido {
        tag1: None,
        tag2: None,
        feedback_uri: None,
        feedback_hash: None,
        ..Default::default()
    };
    let pago = |op: &str| {
        let mut p = payment_v1(PAYER, WORKER, "Released", 1_000);
        p["operator_id"] = json!(op);
        p.to_string()
    };
    let con = |id: &str| Pedido {
        payment_id: id.into(),
        ..base.clone()
    };
    let primera = medir(&mut c, PAYER, &con("a"), &pago("em"));
    let segunda = medir(&mut c, PAYER, &con("b"), &pago("em"));
    assert_eq!(
        transfers(),
        vec![(
            PAYER.to_string(),
            deposit() - u128::from(segunda) * 10u128.pow(19)
        )]
    );
    let otro_op = medir(&mut c, PAYER, &con("c"), &pago("ab"));
    let resumen_em = record_bytes(1 + 4 + WORKER.len() as u64 + 4 + 2, 44);
    assert_eq!(resumen_em, 109);
    assert_eq!(
        otro_op,
        segunda + resumen_em,
        "el operador nuevo paga su resumen"
    );
    assert_eq!(
        transfers(),
        vec![(
            PAYER.to_string(),
            deposit() - u128::from(otro_op) * 10u128.pow(19)
        )]
    );
    // La primera de todas paga ademas el indice y el resumen global nuevos de la calificada.
    let w = 4 + WORKER.len() as u64;
    let indice_y_global = record_bytes(1 + w, 4 + 4 + (1 + w + 1)) + record_bytes(1 + w, 44);
    assert_eq!(primera, segunda + indice_y_global + resumen_em);
}
