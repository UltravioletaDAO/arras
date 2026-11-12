//! El verificador: el `payment_id` canonico, la llamada cruzada, y `check_payment` con cada respuesta de `get_payment`
//! (el espejo estricto del `Payment` con disputa, la coherencia y las cinco guardas). `real` lo prueba contra el
//! `Payment` de verdad de `contracts/escrow-v2`, y `escrow` contra el escrow mismo en el mock (el borde de la disputa
//! vencida).

use crate::*;
use near_sdk::mock::MockAction;
use near_sdk::serde_json::{Value, json};
use near_sdk::test_utils::{VMContextBuilder, get_created_receipts};
use near_sdk::{NearToken, testing_env};

mod callback;
mod escrow;
mod real;

const CONTRACT: &str = "payment-proof.arras-dev.testnet";
const ESCROW: &str = "v2.arras-dev.testnet";
const PAYER: &str = "agente.testnet";
const WORKER: &str = "worker.testnet";
const OTHER: &str = "tercero.testnet";
/// El `payment_id` de V-1 (base58 de 32 bytes, 44 caracteres).
const PID: &str = "DN5eMA3sNES7xsoy6qvy24cVF6Dmwjxpkvd2b4e8P38M";
const RAZON: &str = "2222222222222222222222222222222222222222222222222222222222222222";

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

fn new_contract() -> PaymentProofVerifier {
    ctx(CONTRACT, Gas::from_tgas(300));
    PaymentProofVerifier::new(acc(ESCROW))
}

/// La `dispute` coherente con `status`: abierta en `Disputed`, resuelta en `Resolved`, `null` en los demas.
fn dispute_de(status: &str) -> Value {
    match status {
        "Disputed" => json!({
            "opened_by": "Payer",
            "reason_hash": RAZON,
            "opened_at": "1793500000000000000",
            "resolved_at": null
        }),
        "Resolved" => json!({
            "opened_by": "Receiver",
            "reason_hash": RAZON,
            "opened_at": "1793500000000000000",
            "resolved_at": "1793510000000000000"
        }),
        _ => Value::Null,
    }
}

/// Un `Payment` del escrow como lo serializa (los 19 campos), con la `dispute` coherente con `status`.
fn pago(payer: &str, receiver: &str, status: &str, released: u128) -> Value {
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
        "storage_charged": "4630000000000000000000",
        "sponsored": false,
        "dispute_deadline": "1793577600000000000",
        "arbiter_window_ns": "604800000000000",
        "dispute": dispute_de(status)
    })
}

fn check(client: &str, v: &Value) -> Result<FeedbackProof, VerifierError> {
    check_payment(&acc(client), Ok(serde_json::to_vec(v).unwrap()))
}

fn check_raw(client: &str, b: &[u8]) -> Result<FeedbackProof, VerifierError> {
    check_payment(&acc(client), Ok(b.to_vec()))
}

/// Un pago `Released` valido con `campo` cambiado.
fn con(campo: &str, valor: Value) -> Value {
    let mut p = pago(PAYER, WORKER, "Released", 1_000_000);
    p[campo] = valor;
    p
}

/// Un pago `Resolved` valido (con la `dispute` resuelta) con `campo` de la disputa cambiado.
fn con_disputa(campo: &str, valor: Value) -> Value {
    let mut p = pago(PAYER, WORKER, "Resolved", 400_000);
    p["refunded"] = json!("600000");
    p["dispute"][campo] = valor;
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

const ESTADOS: [&str; 7] = [
    "Authorized",
    "PartiallyReleased",
    "Released",
    "Refunded",
    "Reclaimed",
    "Disputed",
    "Resolved",
];

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

/// El orden de v1: la proof se mira antes que el gas.
#[test]
#[should_panic(expected = "ERC8004_ARV_E02")]
fn proof_no_canonica_con_gas_de_menos_es_e02() {
    let c = new_contract();
    ctx(OTHER, MIN_GAS_VERIFY.saturating_sub(Gas::from_gas(1)));
    c.verify_feedback_proof(acc(PAYER), String::new()).detach();
}

#[test]
#[should_panic(expected = "ERC8004_ARV_E01")]
fn el_propio_verificador_como_escrow_es_e01() {
    ctx(CONTRACT, Gas::from_tgas(300));
    PaymentProofVerifier::new(acc(CONTRACT));
}

#[test]
fn la_otra_parte_y_su_rol() {
    let p = pago(PAYER, WORKER, "Released", 750_000);
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
    // El fee recipient no es parte del trato.
    assert_eq!(
        check("em-fees.arras-dev.testnet", &p),
        Err(VerifierError::E15NoEsParte)
    );
    // Una subcuenta de una parte no es esa parte (la comparacion es de `AccountId` entero, no de sufijo ni de prefijo):
    // la controla quien controla la cuenta padre, pero el pago lo hizo la otra.
    for sub in [
        format!("x.{PAYER}"),
        format!("x.{WORKER}"),
        format!("{PAYER}x"),
        "testnet".to_string(),
    ] {
        assert_eq!(check(&sub, &p), Err(VerifierError::E15NoEsParte), "{sub}");
    }
    // Y al reves: las partes son subcuentas del que califica.
    let hijas = pago(&format!("x.{PAYER}"), &format!("x.{WORKER}"), "Released", 1);
    for padre in [PAYER, WORKER] {
        assert_eq!(
            check(padre, &hijas),
            Err(VerifierError::E15NoEsParte),
            "{padre}"
        );
    }
}

#[test]
fn vale_con_plata_liberada_en_cualquier_estado() {
    for status in ESTADOS {
        assert_eq!(
            check(PAYER, &pago(PAYER, WORKER, status, 1)).map(|p| p.weight),
            Ok(U128(1)),
            "{status}"
        );
        assert_eq!(
            check(PAYER, &pago(PAYER, WORKER, status, 0)),
            Err(VerifierError::E13PagoSinLiberar),
            "{status}"
        );
    }
}

/// La decision sobre disputas (doc del crate): lo liberado antes de una disputa vale mientras esta abierta, y despues de
/// resolverla vale lo que llego al receiver; todo de vuelta al payer sin liberacion previa no vale.
#[test]
fn disputas() {
    // Liberacion parcial de 300.000 y despues una disputa: lo liberado ya es del receiver.
    let abierta = pago(PAYER, WORKER, "Disputed", 300_000);
    for (client, counterparty, role) in [(PAYER, WORKER, "receiver"), (WORKER, PAYER, "payer")] {
        assert_eq!(
            check(client, &abierta),
            Ok(FeedbackProof {
                counterparty: acc(counterparty),
                role: role.into(),
                context: "em".into(),
                weight: U128(300_000)
            })
        );
    }
    // Abierta sin nada liberado: E13 (como un pago Authorized).
    assert_eq!(
        check(PAYER, &pago(PAYER, WORKER, "Disputed", 0)),
        Err(VerifierError::E13PagoSinLiberar)
    );
    // Resuelta: 300.000 de antes + 200.000 del arbitro; los otros 500.000 al payer.
    let mut resuelta = pago(PAYER, WORKER, "Resolved", 500_000);
    resuelta["refunded"] = json!("500000");
    assert_eq!(check(WORKER, &resuelta).unwrap().weight, U128(500_000));
    // Resuelta todo al payer, sin liberacion previa (el arbitro o el plazo vencido): E13, como un refund.
    let mut al_payer = pago(PAYER, WORKER, "Resolved", 0);
    al_payer["refunded"] = json!("1000000");
    for client in [PAYER, WORKER] {
        assert_eq!(
            check(client, &al_payer),
            Err(VerifierError::E13PagoSinLiberar)
        );
    }
    // Resuelta todo al receiver por el arbitro, dentro de su plazo (la vencida es `E16`: la siguiente prueba).
    let al_receiver = pago(PAYER, WORKER, "Resolved", 1_000_000);
    assert_eq!(check(PAYER, &al_receiver).unwrap().weight, U128(1_000_000));
    // Mientras esta abierta, quien la abrio no cambia nada (lo liberado lo libero un capturer antes de abrirla).
    for quien in ["Payer", "Receiver"] {
        let mut p = abierta.clone();
        p["dispute"]["opened_by"] = json!(quien);
        assert!(check(PAYER, &p).is_ok(), "{quien}");
    }
    // Resuelta por el arbitro, tampoco.
    for quien in ["Payer", "Receiver"] {
        let mut p = al_receiver.clone();
        p["dispute"]["opened_by"] = json!(quien);
        assert_eq!(
            check(WORKER, &p).unwrap().weight,
            U128(1_000_000),
            "{quien}"
        );
    }
}

const DAY: u64 = 86_400 * 1_000_000_000;
/// `release_deadline` y `arbiter_window_ns` de [`pago`].
const RD: u64 = 1_793_577_600_000_000_000;
const VENTANA: u64 = 604_800_000_000_000;

/// Un pago `Resolved` cuya disputa abrio `quien` en `abierta` y se cerro en `cerrada`, con `released` y `refunded`.
/// `dispute_deadline` = `reclaim_after` (lo que pone el escrow si el `msg` no lo trae): el payer puede abrir despues de
/// `release_deadline`.
fn resuelta(quien: &str, abierta: u64, cerrada: u64, released: u128, refunded: u128) -> Value {
    let mut p = pago(PAYER, WORKER, "Resolved", released);
    p["refunded"] = json!(refunded.to_string());
    p["dispute_deadline"] = p["reclaim_after"].clone();
    p["dispute"]["opened_by"] = json!(quien);
    p["dispute"]["opened_at"] = json!(abierta.to_string());
    p["dispute"]["resolved_at"] = json!(cerrada.to_string());
    p
}

fn e16(v: &Value) {
    for client in [PAYER, WORKER, OTHER] {
        assert_eq!(
            check(client, v),
            Err(VerifierError::E16DisputaVencida),
            "{v}"
        );
    }
}

/// Lo que `resolve_expired_dispute` le dio al receiver no es prueba (`E16`), ni lo liberado antes en ese pago; lo que
/// decidio el arbitro en su plazo si, y la vencida a favor del payer vale por lo liberado antes.
#[test]
fn la_disputa_vencida_a_favor_del_receiver_es_e16() {
    let abierta = RD - DAY;
    let vence = abierta + VENTANA;
    // El payer la abrio antes de release_deadline y nadie arbitro: lo vivo fue al receiver. El borde exacto
    // (`resolved_at == opened_at + arbiter_window_ns`) es la vencida: el arbitro ya no podia (`E65`).
    for cerrada in [vence, vence + 1, u64::MAX] {
        e16(&resuelta("Payer", abierta, cerrada, 1_000_000, 0));
        // Con una liberacion parcial antes (300.000 del capturer y 700.000 del plazo): el `Payment` queda igual, y
        // aunque quedara distinto no separa una parte de la otra.
        e16(&resuelta("Payer", abierta, cerrada, 300_000, 0));
    }
    // Un nanosegundo antes del borde la cerro el arbitro (la vencida daba `E64`): vale por lo que llego al receiver.
    for (released, refunded) in [(1_000_000, 0), (500_000, 500_000)] {
        let p = resuelta("Payer", abierta, vence - 1, released, refunded);
        for (client, counterparty) in [(PAYER, WORKER), (WORKER, PAYER)] {
            assert_eq!(
                check(client, &p).map(|f| (f.counterparty, f.weight)),
                Ok((acc(counterparty), U128(released))),
                "{released}"
            );
        }
        assert_eq!(check(OTHER, &p), Err(VerifierError::E15NoEsParte));
    }
    // Vencida a favor del payer: el plazo no le dio nada al receiver; vale lo que un capturer libero antes (300.000) o
    // es E13. La abrio el receiver, o el payer en o despues de release_deadline (el borde: `opened_at ==
    // release_deadline` ya es del payer).
    for (quien, abierta) in [
        ("Receiver", RD - DAY),
        ("Receiver", RD),
        ("Payer", RD),
        ("Payer", RD + 1),
        ("Payer", RD + DAY / 2),
    ] {
        let vence = abierta + VENTANA;
        let p = resuelta(quien, abierta, vence, 300_000, 700_000);
        assert_eq!(
            check(WORKER, &p).map(|f| f.weight),
            Ok(U128(300_000)),
            "{quien} {abierta}"
        );
        assert_eq!(
            check(PAYER, &resuelta(quien, abierta, vence, 0, 1_000_000)),
            Err(VerifierError::E13PagoSinLiberar),
            "{quien} {abierta}"
        );
    }
    // Un nanosegundo antes de release_deadline todavia es del receiver.
    e16(&resuelta("Payer", RD - 1, RD - 1 + VENTANA, 1_000_000, 0));
    // El plazo es el congelado en el pago: con otro `arbiter_window_ns`, el borde se mueve con el.
    let mut corta = resuelta("Payer", abierta, abierta + 60_000_000_000, 1_000_000, 0);
    corta["arbiter_window_ns"] = json!("60000000000");
    e16(&corta);
    corta["dispute"]["resolved_at"] = json!((abierta + 59_999_999_999).to_string());
    assert!(check(PAYER, &corta).is_ok());
    // La suma satura como en el escrow (`disputed_until`): con `opened_at + arbiter_window_ns` por encima de u64, el
    // plazo no vence nunca antes de u64::MAX y la cerro el arbitro.
    let mut tarde = resuelta("Payer", u64::MAX - 1, u64::MAX - 1, 1_000_000, 0);
    tarde["release_deadline"] = json!(u64::MAX.to_string());
    tarde["arbiter_window_ns"] = json!("2");
    assert!(check(PAYER, &tarde).is_ok());
    tarde["dispute"]["resolved_at"] = json!(u64::MAX.to_string());
    e16(&tarde);
    // Una disputa abierta del payer antes de release_deadline todavia no vencio nada: vale lo liberado antes.
    let mut abierta_payer = pago(PAYER, WORKER, "Disputed", 300_000);
    abierta_payer["dispute"]["opened_at"] = json!((RD - DAY).to_string());
    assert_eq!(check(WORKER, &abierta_payer).unwrap().weight, U128(300_000));
}

#[test]
fn el_codigo_de_e16() {
    assert_eq!(
        VerifierError::E16DisputaVencida.code(),
        "E16_DISPUTA_VENCIDA"
    );
    assert!(
        VerifierError::E16DisputaVencida
            .panic_message()
            .starts_with("ERC8004_ARV_E16: ")
    );
}

#[test]
fn payer_igual_a_receiver_es_e14() {
    for status in ESTADOS {
        assert_eq!(
            check(PAYER, &pago(PAYER, PAYER, status, 1)),
            Err(VerifierError::E14PayerEsReceiver),
            "{status}"
        );
    }
    assert_eq!(
        check(OTHER, &pago(PAYER, PAYER, "Released", 1)),
        Err(VerifierError::E14PayerEsReceiver)
    );
}

/// El orden de las guardas: E11 (incoherente) -> E13 -> E16 -> E14 -> E15.
#[test]
fn el_orden_de_las_guardas() {
    let mut incoherente_sin_liberar = pago(PAYER, PAYER, "Disputed", 0);
    incoherente_sin_liberar["dispute"] = Value::Null;
    assert_eq!(
        check(OTHER, &incoherente_sin_liberar),
        Err(VerifierError::E11RespuestaInvalida)
    );
    assert_eq!(
        check(OTHER, &pago(PAYER, PAYER, "Released", 0)),
        Err(VerifierError::E13PagoSinLiberar)
    );
    assert_eq!(
        check(OTHER, &pago(PAYER, PAYER, "Released", 1)),
        Err(VerifierError::E14PayerEsReceiver)
    );
    // La vencida a favor del receiver: despues de E13 y antes de E14 y E15.
    let abierta = RD - DAY;
    assert_eq!(
        check(OTHER, &resuelta("Payer", abierta, abierta + VENTANA, 0, 0)),
        Err(VerifierError::E13PagoSinLiberar)
    );
    let mut auto = resuelta("Payer", abierta, abierta + VENTANA, 1_000_000, 0);
    auto["receiver"] = json!(PAYER);
    assert_eq!(check(OTHER, &auto), Err(VerifierError::E16DisputaVencida));
    assert_eq!(check(PAYER, &auto), Err(VerifierError::E16DisputaVencida));
}

#[test]
fn null_es_e12_fallo_es_e10_y_demasiado_largo_es_e11() {
    assert_eq!(
        check_raw(PAYER, b"null"),
        Err(VerifierError::E12PagoInexistente)
    );
    assert_eq!(
        check_payment(&acc(PAYER), Err(PromiseError::Failed)),
        Err(VerifierError::E10GetPaymentFallo)
    );
    assert_eq!(
        check_payment(
            &acc(PAYER),
            Err(PromiseError::TooLong(MAX_PAYMENT_RESULT_LEN + 1))
        ),
        Err(VerifierError::E11RespuestaInvalida)
    );
}

const CAMPOS: [&str; 19] = [
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
    "dispute_deadline",
    "arbiter_window_ns",
    "dispute",
];

const CAMPOS_DISPUTA: [&str; 4] = ["opened_by", "reason_hash", "opened_at", "resolved_at"];

#[test]
fn los_19_campos_son_los_del_escrow() {
    let p = pago(PAYER, WORKER, "Released", 1);
    let mut claves: Vec<&str> = p.as_object().unwrap().keys().map(String::as_str).collect();
    let mut esperadas = CAMPOS.to_vec();
    claves.sort_unstable();
    esperadas.sort_unstable();
    assert_eq!(claves, esperadas);
}

#[test]
fn un_campo_de_menos_es_e11() {
    for campo in CAMPOS {
        for status in ["Released", "Resolved"] {
            let mut p = pago(PAYER, WORKER, status, 1_000_000);
            p.as_object_mut().unwrap().remove(campo);
            e11(&p);
        }
    }
    for campo in CAMPOS_DISPUTA {
        for status in ["Disputed", "Resolved"] {
            let mut p = pago(PAYER, WORKER, status, 1_000_000);
            p["dispute"].as_object_mut().unwrap().remove(campo);
            e11(&p);
        }
    }
}

#[test]
fn null_solo_vale_en_dispute_y_resolved_at() {
    for campo in CAMPOS {
        if campo != "dispute" {
            e11(&con(campo, Value::Null));
        }
    }
    for campo in CAMPOS_DISPUTA {
        if campo != "resolved_at" {
            e11(&con_disputa(campo, Value::Null));
        }
    }
    // `dispute: null` y `resolved_at: null` valen donde el escrow los escribe (Released y Disputed).
    assert!(check(PAYER, &pago(PAYER, WORKER, "Released", 1)).is_ok());
    assert!(check(PAYER, &pago(PAYER, WORKER, "Disputed", 1)).is_ok());
}

#[test]
fn un_campo_de_mas_es_e11() {
    for (campo, valor) in [
        ("budget", json!(null)),
        ("arbiter", json!("arbiter.testnet")),
        ("resolve_by", json!("1793577600000000000")),
        ("Status", json!("Released")),
        ("dispute_deadline ", json!("1")),
    ] {
        e11(&con(campo, valor));
    }
    for (campo, valor) in [
        ("amount", json!("1")),
        ("expired", json!(true)),
        ("resolvedAt", json!(null)),
    ] {
        e11(&con_disputa(campo, valor));
    }
}

#[test]
fn un_pago_de_v1_es_e11() {
    // Los 16 campos de v1: le faltan dispute_deadline, arbiter_window_ns y dispute.
    let mut p = pago(PAYER, WORKER, "Released", 1_000_000);
    for campo in ["dispute_deadline", "arbiter_window_ns", "dispute"] {
        p.as_object_mut().unwrap().remove(campo);
    }
    assert_eq!(p.as_object().unwrap().len(), 16);
    e11(&p);
}

#[test]
fn una_clave_repetida_es_e11() {
    let s = pago(PAYER, WORKER, "Released", 1).to_string();
    let rep = s.replacen('{', &format!("{{\"payer\":\"{OTHER}\","), 1);
    let disputa = pago(PAYER, WORKER, "Disputed", 1).to_string();
    let rep_disputa = disputa.replacen(
        "\"opened_by\":\"Payer\"",
        "\"opened_by\":\"Payer\",\"opened_by\":\"Receiver\"",
        1,
    );
    assert_ne!(rep_disputa, disputa);
    let rep_dispute = disputa.replacen('{', "{\"dispute\":null,", 1);
    for raw in [&rep, &rep_disputa, &rep_dispute] {
        for client in [PAYER, WORKER, OTHER] {
            assert_eq!(
                check_raw(client, raw.as_bytes()),
                Err(VerifierError::E11RespuestaInvalida),
                "{raw}"
            );
        }
    }
}

#[test]
fn un_status_que_no_es_del_escrow_es_e11() {
    for s in [
        json!("Open"),
        json!("Expired"),
        json!("disputed"),
        json!("released"),
        json!(" Released"),
        json!("Released "),
        json!(""),
        json!({"Released": null}),
        json!(["Released"]),
        json!(2),
        json!(true),
        Value::Null,
        json!("Resolved "),
        json!("resolved"),
        json!("Dispute"),
    ] {
        // Con cada forma de `dispute` (sin, abierta y resuelta): un status de mas que el espejo mapeara a cualquiera
        // de los 7 seria coherente con alguna, asi que lo frena el espejo y no la coherencia.
        for forma in ["Released", "Disputed", "Resolved"] {
            let mut p = pago(PAYER, WORKER, forma, 1);
            p["status"] = s.clone();
            e11(&p);
        }
    }
}

#[test]
fn un_opened_by_que_no_es_del_escrow_es_e11() {
    for s in [
        json!("payer"),
        json!("PAYER"),
        json!("Arbiter"),
        json!(""),
        json!(0),
        json!({"Payer": null}),
    ] {
        e11(&con_disputa("opened_by", s));
    }
}

#[test]
fn un_numero_no_canonico_es_e11() {
    let u128_fuera = "340282366920938463463374607431768211456";
    let u64_fuera = "18446744073709551616";
    let malos = |fuera: &str| {
        vec![
            json!("+1"),
            json!("01"),
            json!("-1"),
            json!(" 1"),
            json!("1 "),
            json!(""),
            json!("1e0"),
            json!("0x1"),
            json!(1),
            json!(fuera),
        ]
    };
    for (campo, fuera) in [
        ("amount", u128_fuera),
        ("released", u128_fuera),
        ("refunded", u128_fuera),
        ("storage_charged", u128_fuera),
        ("created_at", u64_fuera),
        ("release_deadline", u64_fuera),
        ("reclaim_after", u64_fuera),
        ("dispute_deadline", u64_fuera),
        ("arbiter_window_ns", u64_fuera),
    ] {
        for v in malos(fuera) {
            e11(&con(campo, v));
        }
    }
    for campo in ["opened_at", "resolved_at"] {
        for v in malos(u64_fuera) {
            e11(&con_disputa(campo, v));
        }
    }
    // `resolved_at` no canonico tambien en una disputa abierta: si el espejo lo leyera como `null`, la coherencia no
    // lo frenaria (de la ronda del refutador).
    for v in malos(u64_fuera) {
        let mut p = pago(PAYER, WORKER, "Disputed", 1);
        p["dispute"]["resolved_at"] = v;
        e11(&p);
    }
}

#[test]
fn un_hash_que_no_es_hex_de_64_en_minusculas_es_e11() {
    let bueno = "77a2c0d6b3e9c3c5a6a7b48e1e8f6d7c0b6d1f6e4d0a7c2b9e8f7a6b5c4d3e2f";
    let malos = [
        json!(bueno.to_uppercase()),
        json!(&bueno[1..]),
        json!(format!("{bueno}0")),
        json!(format!("0x{}", &bueno[2..])),
        json!(format!("g{}", &bueno[1..])),
        json!(format!(" {}", &bueno[1..])),
        json!(""),
        json!(7),
    ];
    for v in &malos {
        e11(&con("terms_hash", v.clone()));
        e11(&con_disputa("reason_hash", v.clone()));
    }
    assert!(check(PAYER, &con("terms_hash", json!("0".repeat(64)))).is_ok());
}

#[test]
fn los_demas_tipos_son_estrictos() {
    for (campo, v) in [
        ("fee_bps", json!("1300")),
        ("fee_bps", json!(65536)),
        ("fee_bps", json!(1300.0)),
        ("fee_bps_max", json!(-1)),
        ("sponsored", json!("false")),
        ("sponsored", json!(0)),
        ("fee_recipient", json!("NO VALIDA")),
        ("payer", json!("")),
        ("receiver", json!(7)),
        ("dispute", json!(false)),
        ("dispute", json!([])),
        ("dispute", json!({})),
        ("dispute", json!("Payer")),
    ] {
        e11(&con(campo, v));
    }
}

#[test]
fn operator_id_fuera_del_charset_es_e11() {
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
    for b in [&b""[..], b"{", b"[]", b"true", b"\"x\"", b"{}", b"0"] {
        assert_eq!(
            check_raw(PAYER, b),
            Err(VerifierError::E11RespuestaInvalida)
        );
    }
}

/// `status` y `dispute` tienen que ir juntos como los escribe el escrow; cualquier otra combinacion es `E11`.
#[test]
fn status_y_dispute_coherentes() {
    let abierta = dispute_de("Disputed");
    let resuelta = dispute_de("Resolved");
    for status in ESTADOS {
        for (nombre, d) in [
            ("null", Value::Null),
            ("abierta", abierta.clone()),
            ("resuelta", resuelta.clone()),
        ] {
            let mut p = pago(PAYER, WORKER, status, 1);
            p["dispute"] = d;
            let coherente = matches!(
                (status, nombre),
                ("Disputed", "abierta") | ("Resolved", "resuelta")
            ) || (!matches!(status, "Disputed" | "Resolved") && nombre == "null");
            assert_eq!(
                check(PAYER, &p).is_ok(),
                coherente,
                "{status} con dispute {nombre}"
            );
            if !coherente {
                e11(&p);
            }
        }
    }
}

#[test]
fn liberado_mas_devuelto_no_pasa_de_amount() {
    let mut p = pago(PAYER, WORKER, "Refunded", 400_000);
    p["refunded"] = json!("600000");
    assert!(check(PAYER, &p).is_ok());
    p["refunded"] = json!("600001");
    e11(&p);
    let mut p = pago(PAYER, WORKER, "Released", 1_000_001);
    e11(&p);
    // Sin desbordar: released = u128::MAX y refunded = 1.
    p["amount"] = json!(u128::MAX.to_string());
    p["released"] = json!(u128::MAX.to_string());
    p["refunded"] = json!("1");
    e11(&p);
    p["refunded"] = json!("0");
    assert_eq!(check(PAYER, &p).unwrap().weight, U128(u128::MAX));
}

#[test]
fn los_bordes_valen() {
    let mut p = pago(PAYER, WORKER, "Resolved", u128::MAX);
    p["amount"] = json!(u128::MAX.to_string());
    p["refunded"] = json!("0");
    p["created_at"] = json!("0");
    p["release_deadline"] = json!(u64::MAX.to_string());
    p["dispute_deadline"] = json!(u64::MAX.to_string());
    p["arbiter_window_ns"] = json!("0");
    p["dispute"]["opened_at"] = json!("0");
    p["dispute"]["resolved_at"] = json!(u64::MAX.to_string());
    p["fee_bps"] = json!(0);
    p["fee_bps_max"] = json!(u16::MAX);
    p["sponsored"] = json!(true);
    assert_eq!(check(PAYER, &p).unwrap().weight, U128(u128::MAX));
}

#[test]
fn config() {
    let c = new_contract();
    let v = serde_json::to_value(c.get_config()).unwrap();
    assert_eq!(
        v,
        json!({
            "contract": "erc8004-arras-verifier",
            "escrow": ESCROW,
            "min_gas_verify": "25000000000000",
            "gas_get_payment": "10000000000000",
            "gas_on_payment": "10000000000000",
        })
    );
}
