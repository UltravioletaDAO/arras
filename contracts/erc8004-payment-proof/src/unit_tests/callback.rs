//! `on_payment` con un resultado de promesa de verdad (el tope de 8 KiB y el indice 0, que si no solo cubre el
//! sandbox), el `Payment` real mas largo, cuentas implicitas, y lo que el espejo a proposito no exige (de la ronda del
//! refutador).

use super::*;
use arras_escrow_v2::{Dispute, Party, Payment, PaymentStatus};
use near_sdk::PromiseResult;

fn ctx_cb(r: PromiseResult) {
    testing_env!(
        VMContextBuilder::new()
            .current_account_id(acc(CONTRACT))
            .predecessor_account_id(acc(CONTRACT))
            .prepaid_gas(Gas::from_tgas(10))
            .build(),
        near_sdk::test_vm_config(),
        near_sdk::RuntimeFeesConfig::test(),
        Default::default(),
        vec![r]
    );
}

/// El `Payment` mas largo que el escrow puede escribir: cuentas de 64, operator de 32, u128/u64 maximos, disputa resuelta.
fn peor_caso() -> Payment {
    let a = |c: char| acc(&c.to_string().repeat(64));
    Payment {
        operator_id: "o".repeat(32),
        payer: a('a'),
        receiver: a('b'),
        amount: U128(u128::MAX),
        released: U128(u128::MAX - 1),
        refunded: U128(1),
        fee_bps: 1800,
        fee_recipient: a('c'),
        fee_bps_max: u16::MAX,
        created_at: U64(u64::MAX),
        release_deadline: U64(u64::MAX),
        reclaim_after: U64(u64::MAX),
        terms_hash: "f".repeat(64),
        status: PaymentStatus::Resolved,
        storage_charged: U128(u128::MAX),
        sponsored: true,
        dispute_deadline: U64(u64::MAX),
        arbiter_window_ns: U64(u64::MAX),
        dispute: Some(Dispute {
            opened_by: Party::Receiver,
            reason_hash: "e".repeat(64),
            opened_at: U64(u64::MAX),
            resolved_at: Some(U64(u64::MAX)),
        }),
    }
}

#[test]
fn on_payment_con_el_payment_real_mas_largo() {
    let b = serde_json::to_vec(&peor_caso()).unwrap();
    // 1.007 bytes, el peor caso: un mutante del tope por debajo de eso (p. ej. 1000) seria un falso negativo para
    // siempre y hoy solo lo veria un pago real tan largo; este test lo mata.
    assert_eq!(b.len(), 1007);
    assert!(b.len() < MAX_PAYMENT_RESULT_LEN);
    let c = new_contract();
    ctx_cb(PromiseResult::Successful(b));
    let fp = c.on_payment(acc(&"a".repeat(64)));
    assert_eq!(fp.weight, U128(u128::MAX - 1));
    assert_eq!(fp.role, "receiver");
}

#[test]
fn on_payment_en_el_tope_exacto_y_un_byte_mas() {
    let c = new_contract();
    let mut b = serde_json::to_vec(&pago_parcial()).unwrap();
    // El tope es 8 KiB justos (literal, no la constante: asi un cambio del tope tambien pone el test en rojo).
    assert_eq!(MAX_PAYMENT_RESULT_LEN, 8192);
    b.resize(8192, b' ');
    ctx_cb(PromiseResult::Successful(b.clone()));
    assert_eq!(c.on_payment(acc(PAYER)).weight, U128(400_000));
    b.push(b' ');
    ctx_cb(PromiseResult::Successful(b));
    let r = std::panic::catch_unwind(|| c.on_payment(acc(PAYER)));
    let msg = r
        .err()
        .and_then(|e| e.downcast::<String>().ok())
        .map(|m| *m)
        .unwrap_or_default();
    assert!(msg.contains("ERC8004_ARV_E11"), "{msg}");
}

fn pago_parcial() -> Payment {
    let mut p = peor_caso();
    p.payer = acc(PAYER);
    p.receiver = acc(WORKER);
    p.amount = U128(1_000_000);
    p.released = U128(400_000);
    p.refunded = U128(0);
    p.status = PaymentStatus::PartiallyReleased;
    p.dispute = None;
    p
}

#[test]
fn cuentas_implicitas_near_y_eth() {
    let mut p = pago_parcial();
    p.payer = acc("0x1234567890abcdef1234567890abcdef12345678");
    p.receiver = acc(&"0123456789abcdef".repeat(4));
    let b = serde_json::to_vec(&p).unwrap();
    let fp = check_payment(&p.payer, Ok(b)).unwrap();
    assert_eq!(fp.counterparty, p.receiver);
}

/// Lo que el escrow no escribe nunca y el espejo acepta igual, a proposito (el estado no decide nada, y en un contrato
/// inmutable cada guarda de mas es un falso negativo posible para siempre): `Authorized` con plata liberada, `Released`
/// con algo vivo, `Resolved` con `released + refunded < amount` o `resolved_at` antes de `opened_at`. El escrow no tiene
/// llaves y su codigo esta fijado: esas respuestas no llegan.
#[test]
fn el_estado_no_decide_lo_que_el_escrow_no_escribe() {
    for (status, released, refunded, dispute) in [
        (PaymentStatus::Authorized, 1, 0, None),
        (PaymentStatus::Released, 1, 0, None),
        (
            PaymentStatus::Resolved,
            1,
            0,
            Some(Dispute {
                opened_by: Party::Payer,
                reason_hash: "e".repeat(64),
                opened_at: U64(5),
                resolved_at: Some(U64(1)),
            }),
        ),
    ] {
        let mut p = pago_parcial();
        p.status = status;
        p.released = U128(released);
        p.refunded = U128(refunded);
        p.dispute = dispute;
        let b = serde_json::to_vec(&p).unwrap();
        assert!(check_payment(&acc(PAYER), Ok(b)).is_ok(), "{status:?}");
    }
}
