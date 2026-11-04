//! El verificador: el `payment_id` canonico, la llamada cruzada, y `check_payment` con cada respuesta de `get_payment`
//! (los casos del espejo estricto de registry-v1, RONDA 1).

use crate::*;
use near_sdk::mock::MockAction;
use near_sdk::serde_json::{Value, json};
use near_sdk::test_utils::{VMContextBuilder, get_created_receipts};
use near_sdk::{NearToken, testing_env};

const CONTRACT: &str = "erc8004-verifier.arras-dev.testnet";
const ESCROW: &str = "arras-v1b-1101.testnet";
const PAYER: &str = "agente.testnet";
const WORKER: &str = "worker.testnet";
const OTHER: &str = "tercero.testnet";
/// El `payment_id` de V-1 (base58 de 32 bytes, 44 caracteres).
const PID: &str = "DN5eMA3sNES7xsoy6qvy24cVF6Dmwjxpkvd2b4e8P38M";

fn acc(s: &str) -> AccountId {
    s.parse().unwrap()
}

fn ctx(predecessor: &str, gas: Gas) {
    testing_env!(
        VMContextBuilder::new()
            .current_account_id(acc(CONTRACT))
            .predecessor_account_id(acc(predecessor))
            .attached_deposit(NearToken::from_yoctonear(0))
            .prepaid_gas(gas)
            .build()
    );
}

fn new_contract() -> ArrasVerifier {
    ctx(CONTRACT, Gas::from_tgas(300));
    ArrasVerifier::new(acc(ESCROW))
}

fn payment_v1(payer: &str, receiver: &str, status: &str, released: u128) -> Value {
    json!({
        "operator_id": "em",
        "payer": payer,
        "receiver": receiver,
        "amount": "1000000",
        "released": released.to_string(),
        "refunded": "0",
        "fee_bps": 1300,
        "fee_recipient": "em-fees.arras-dev.testnet",
        "fee_bps_max": 1800,
        "created_at": "1793491200000000000",
        "release_deadline": "1793577600000000000",
        "reclaim_after": "1793664000000000000",
        "terms_hash": "77a2c0d6b3e9c3c5a6a7b48e1e8f6d7c0b6d1f6e4d0a7c2b9e8f7a6b5c4d3e2f",
        "status": status,
        "storage_charged": "3770000000000000000000",
        "sponsored": false
    })
}

fn check(client: &str, v: &Value) -> Result<FeedbackProof, VerifierError> {
    check_payment(&acc(client), Ok(serde_json::to_vec(v).unwrap()))
}

fn check_raw(client: &str, b: &[u8]) -> Result<FeedbackProof, VerifierError> {
    check_payment(&acc(client), Ok(b.to_vec()))
}

fn con(campo: &str, valor: Value) -> Value {
    let mut p = payment_v1(PAYER, WORKER, "Released", 1_000_000);
    p[campo] = valor;
    p
}

fn e11(v: &Value) {
    for client in [PAYER, WORKER] {
        assert_eq!(
            check(client, v),
            Err(VerifierError::E11RespuestaInvalida),
            "{v}"
        );
    }
}

#[test]
fn payment_id_canonico() {
    assert!(is_canonical_payment_id(PID));
    // Un id de 43 caracteres tambien puede ser canonico (otros 32 bytes): no hay largo fijo.
    assert!(is_canonical_payment_id(&PID[1..]));
    // 32 bytes que empiezan con ceros: '1' adelante, y mas corto.
    let ceros = bs58::encode([0u8; 32]).into_string();
    assert_eq!(ceros, "1".repeat(32));
    assert!(is_canonical_payment_id(&ceros));
    for bad in [
        "",
        "0OIl",
        &format!("{PID}1"),
        &format!("1{PID}"),
        &"1".repeat(33),
        &"1".repeat(31),
        &format!(" {PID}"),
        &PID.to_lowercase(),
    ] {
        assert!(!is_canonical_payment_id(bad), "{bad:?}");
    }
}

#[test]
fn verify_llama_get_payment_y_el_callback() {
    let c = new_contract();
    ctx(OTHER, Gas::from_tgas(50));
    c.verify_feedback_proof(acc(PAYER), PID.into()).detach();
    let mut calls = vec![];
    for r in get_created_receipts() {
        for a in r.actions {
            if let MockAction::FunctionCallWeight {
                method_name,
                args,
                prepaid_gas,
                attached_deposit,
                ..
            } = a
            {
                calls.push((
                    r.receiver_id.to_string(),
                    String::from_utf8(method_name).unwrap(),
                    serde_json::from_slice::<Value>(&args).unwrap(),
                    prepaid_gas,
                    attached_deposit.as_yoctonear(),
                ));
            }
        }
    }
    assert_eq!(calls.len(), 2);
    assert_eq!(
        calls[0],
        (
            ESCROW.to_string(),
            "get_payment".to_string(),
            json!({"payment_id": PID}),
            GAS_GET_PAYMENT,
            0
        )
    );
    assert_eq!(
        calls[1],
        (
            CONTRACT.to_string(),
            "on_payment".to_string(),
            json!({"client": PAYER}),
            GAS_ON_PAYMENT,
            0
        )
    );
}

#[test]
#[should_panic(expected = "ERC8004_ARV_E02")]
fn proof_no_canonica_es_e02() {
    let c = new_contract();
    ctx(OTHER, Gas::from_tgas(50));
    c.verify_feedback_proof(acc(PAYER), format!("1{PID}"))
        .detach();
}

#[test]
#[should_panic(expected = "ERC8004_ARV_E03")]
fn gas_de_menos_es_e03() {
    let c = new_contract();
    ctx(OTHER, MIN_GAS_VERIFY.saturating_sub(Gas::from_gas(1)));
    c.verify_feedback_proof(acc(PAYER), PID.into()).detach();
}

#[test]
#[should_panic(expected = "ERC8004_ARV_E01")]
fn el_propio_verificador_como_escrow_es_e01() {
    ctx(CONTRACT, Gas::from_tgas(300));
    ArrasVerifier::new(acc(CONTRACT));
}

#[test]
fn la_otra_parte_y_su_rol() {
    let p = payment_v1(PAYER, WORKER, "Released", 750_000);
    assert_eq!(
        check(PAYER, &p),
        Ok(FeedbackProof {
            counterparty: acc(WORKER),
            role: "receiver".into(),
            context: "em".into(),
            weight: U128(750_000)
        })
    );
    assert_eq!(
        check(WORKER, &p),
        Ok(FeedbackProof {
            counterparty: acc(PAYER),
            role: "payer".into(),
            context: "em".into(),
            weight: U128(750_000)
        })
    );
    assert_eq!(check(OTHER, &p), Err(VerifierError::E15NoEsParte));
}

#[test]
fn vale_con_plata_liberada_en_cualquier_estado() {
    for status in [
        "Authorized",
        "PartiallyReleased",
        "Released",
        "Refunded",
        "Reclaimed",
    ] {
        assert!(
            check(PAYER, &payment_v1(PAYER, WORKER, status, 1)).is_ok(),
            "{status}"
        );
        assert_eq!(
            check(PAYER, &payment_v1(PAYER, WORKER, status, 0)),
            Err(VerifierError::E13PagoSinLiberar),
            "{status}"
        );
    }
}

#[test]
fn payer_igual_a_receiver_es_e14() {
    assert_eq!(
        check(PAYER, &payment_v1(PAYER, PAYER, "Released", 1)),
        Err(VerifierError::E14PayerEsReceiver)
    );
}

#[test]
fn null_es_e12_y_fallo_es_e10() {
    assert_eq!(
        check_raw(PAYER, b"null"),
        Err(VerifierError::E12PagoInexistente)
    );
    assert_eq!(
        check_payment(&acc(PAYER), Err(PromiseError::Failed)),
        Err(VerifierError::E10GetPaymentFallo)
    );
}

const CAMPOS_V1: [&str; 16] = [
    "operator_id",
    "payer",
    "receiver",
    "amount",
    "released",
    "refunded",
    "fee_bps",
    "fee_recipient",
    "fee_bps_max",
    "created_at",
    "release_deadline",
    "reclaim_after",
    "terms_hash",
    "status",
    "storage_charged",
    "sponsored",
];

#[test]
fn un_campo_de_menos_o_en_null_es_e11() {
    for campo in CAMPOS_V1 {
        let mut p = payment_v1(PAYER, WORKER, "Released", 1_000_000);
        p.as_object_mut().unwrap().remove(campo);
        e11(&p);
        e11(&con(campo, Value::Null));
    }
}

#[test]
fn un_campo_de_mas_es_e11() {
    for (campo, valor) in [
        ("dispute_deadline", json!("1793577600000000000")),
        ("arbiter_window_ns", json!("86400000000000")),
        ("dispute", Value::Null),
        ("Status", json!("Released")),
    ] {
        e11(&con(campo, valor));
    }
}

#[test]
fn una_clave_repetida_es_e11() {
    let s = payment_v1(PAYER, WORKER, "Released", 1).to_string();
    let rep = s.replacen('{', &format!("{{\"payer\":\"{OTHER}\","), 1);
    for client in [PAYER, WORKER, OTHER] {
        assert_eq!(
            check_raw(client, rep.as_bytes()),
            Err(VerifierError::E11RespuestaInvalida)
        );
    }
}

#[test]
fn un_status_que_no_es_de_v1_es_e11() {
    for s in [
        json!("Disputed"),
        json!("Resolved"),
        json!("released"),
        json!(" Released"),
        json!(""),
        json!({"Released": null}),
        json!(["Released"]),
        json!(2),
        json!(true),
    ] {
        e11(&con("status", s));
    }
}

#[test]
fn un_numero_no_canonico_es_e11() {
    let u128_fuera = "340282366920938463463374607431768211456";
    let u64_fuera = "18446744073709551616";
    for (campo, fuera) in [
        ("amount", u128_fuera),
        ("released", u128_fuera),
        ("refunded", u128_fuera),
        ("storage_charged", u128_fuera),
        ("created_at", u64_fuera),
        ("release_deadline", u64_fuera),
        ("reclaim_after", u64_fuera),
    ] {
        for v in [
            json!("+1"),
            json!("01"),
            json!("-1"),
            json!(" 1"),
            json!(""),
            json!("1e0"),
            json!(1),
            json!(fuera),
        ] {
            e11(&con(campo, v));
        }
    }
}

#[test]
fn los_demas_tipos_son_estrictos() {
    for (campo, v) in [
        ("fee_bps", json!("1300")),
        ("fee_bps", json!(65536)),
        ("fee_bps_max", json!(-1)),
        ("sponsored", json!("false")),
        ("fee_recipient", json!("NO VALIDA")),
        ("payer", json!("")),
        ("receiver", json!(7)),
        ("terms_hash", json!(7)),
    ] {
        e11(&con(campo, v));
    }
}

#[test]
fn operator_id_fuera_del_charset_de_v1_es_e11() {
    for op in [
        json!(""),
        json!("o".repeat(33)),
        json!("EM"),
        json!("e m"),
        json!("e.m"),
        json!(7),
    ] {
        e11(&con("operator_id", op));
    }
    assert_eq!(
        check(PAYER, &con("operator_id", json!("o".repeat(32))))
            .unwrap()
            .context,
        "o".repeat(32)
    );
}

#[test]
fn respuestas_rotas_son_e11() {
    for b in [&b""[..], b"{", b"[]", b"true", b"\"x\""] {
        assert_eq!(
            check_raw(PAYER, b),
            Err(VerifierError::E11RespuestaInvalida)
        );
    }
}

#[test]
fn los_bordes_valen() {
    let mut p = payment_v1(PAYER, WORKER, "Released", u128::MAX);
    p["amount"] = json!(u128::MAX.to_string());
    p["refunded"] = json!("0");
    p["created_at"] = json!("0");
    p["release_deadline"] = json!(u64::MAX.to_string());
    p["fee_bps"] = json!(0);
    p["fee_bps_max"] = json!(u16::MAX);
    p["sponsored"] = json!(true);
    assert_eq!(check(PAYER, &p).unwrap().weight, U128(u128::MAX));
}

#[test]
fn config() {
    let c = new_contract();
    let v = serde_json::to_value(c.get_config()).unwrap();
    assert_eq!(v["contract"], "erc8004-arras-verifier");
    assert_eq!(v["escrow"], ESCROW);
}
