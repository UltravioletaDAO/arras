//! Vistas: `list_feedback` paginada, `get_summary` (promedio, roles, ultima), `get_feedback*` e `is_feedback_pending`.

use super::common::*;
use crate::*;

fn pid(i: u32) -> String {
    format!("pago-{i:03}")
}

#[test]
fn list_feedback_pagina_en_orden_de_llegada() {
    let mut c = new_contract();
    for i in 0..55u32 {
        let p = Pedido {
            payment_id: pid(i),
            score: (i % 101) as u8,
            ..Default::default()
        };
        assert_eq!(rate(&mut c, PAYER, &p, &released_json()), Some(i));
    }
    let primera = c.list_feedback(acc(WORKER), None, None);
    assert_eq!(primera.len(), 50);
    assert_eq!(c.list_feedback(acc(WORKER), Some(0), Some(100)).len(), 50);
    assert_eq!(primera[0].payment_id, pid(0));
    assert_eq!(primera[49].feedback_id, 49);
    let resto = c.list_feedback(acc(WORKER), Some(50), Some(50));
    assert_eq!(resto.len(), 5);
    assert_eq!(resto[4].payment_id, pid(54));
    assert_eq!(c.list_feedback(acc(WORKER), Some(10), Some(3)).len(), 3);
    assert_eq!(
        c.list_feedback(acc(WORKER), Some(10), Some(3))[0].feedback_id,
        10
    );
    assert!(c.list_feedback(acc(WORKER), Some(55), None).is_empty());
    assert!(
        c.list_feedback(acc(WORKER), Some(u32::MAX), Some(u32::MAX))
            .is_empty()
    );
    assert!(
        c.list_feedback(acc(PAYER), None, None).is_empty(),
        "el rater no recibio"
    );
    assert!(c.list_feedback(acc(OTHER), None, None).is_empty());
    let s = c.get_summary(acc(WORKER));
    assert_eq!(s.count, 55);
    assert_eq!(s.score_sum, U64((0..55u64).sum()));
    assert_eq!(s.average_x100, Some(2700));
    assert_eq!(s.last_feedback_id, Some(54));
    assert_eq!(s.last_at, Some(U64(NOW + 1_000_000_000)));
}

#[test]
fn resumen_vacio() {
    let c = new_contract();
    let s = c.get_summary(acc(OTHER));
    assert_eq!(
        s,
        SummaryView {
            account_id: acc(OTHER),
            count: 0,
            revoked: 0,
            score_sum: U64(0),
            average_x100: None,
            as_payer: RoleSummaryView {
                count: 0,
                revoked: 0,
                score_sum: U64(0),
                average_x100: None,
            },
            as_receiver: RoleSummaryView {
                count: 0,
                revoked: 0,
                score_sum: U64(0),
                average_x100: None,
            },
            last_feedback_id: None,
            last_at: None,
        }
    );
}

#[test]
fn resumen_por_rol_y_promedio_con_dos_decimales() {
    let mut c = new_contract();
    // WORKER recibe 1 y 2 como receiver (de PAYER) y 100 como payer (de OTHER en otro pago donde paga WORKER).
    for (i, score) in [1u8, 2].into_iter().enumerate() {
        let p = Pedido {
            payment_id: pid(i as u32),
            score,
            ..Default::default()
        };
        rate(&mut c, PAYER, &p, &released_json()).unwrap();
    }
    let pago = payment_v1(WORKER, OTHER, "Released", 10).to_string();
    let p = Pedido {
        payment_id: pid(9),
        score: 100,
        ..Default::default()
    };
    assert_eq!(rate(&mut c, OTHER, &p, &pago), Some(2));
    let s = c.get_summary(acc(WORKER));
    assert_eq!(s.as_receiver.count, 2);
    assert_eq!(s.as_receiver.score_sum, U64(3));
    assert_eq!(s.as_receiver.average_x100, Some(150));
    assert_eq!(s.as_payer.count, 1);
    assert_eq!(s.as_payer.average_x100, Some(10000));
    assert_eq!(s.count, 3);
    assert_eq!(s.score_sum, U64(103));
    assert_eq!(s.average_x100, Some(3433), "floor(10300 / 3)");
    assert_eq!(s.last_feedback_id, Some(2));
}

#[test]
fn get_feedback_y_por_pago() {
    let mut c = new_contract();
    assert_eq!(c.get_feedback(0), None);
    rate(&mut c, PAYER, &Pedido::default(), &released_json()).unwrap();
    assert!(c.get_feedback(0).is_some());
    assert_eq!(c.get_feedback(1), None);
    assert_eq!(c.get_feedback(u32::MAX), None);
    assert!(
        c.get_feedback_for_payment(acc(ESCROW), PID.into(), acc(PAYER))
            .is_some()
    );
    assert_eq!(
        c.get_feedback_for_payment(acc(ESCROW), PID.into(), acc(WORKER)),
        None
    );
    assert_eq!(
        c.get_feedback_for_payment(acc(ESCROW2), PID.into(), acc(PAYER)),
        None
    );
    assert_eq!(
        c.get_feedback_for_payment(acc(ESCROW), PID2.into(), acc(PAYER)),
        None
    );
}

#[test]
fn pending_vence_a_los_ttl_bloques() {
    let mut c = new_contract();
    give(&mut c, PAYER, &Pedido::default());
    ctx_at(OTHER, 0, HEIGHT + PENDING_TTL_BLOCKS - 1);
    assert!(c.is_feedback_pending(acc(ESCROW), PID.into(), acc(PAYER)));
    ctx_at(OTHER, 0, HEIGHT + PENDING_TTL_BLOCKS);
    assert!(!c.is_feedback_pending(acc(ESCROW), PID.into(), acc(PAYER)));
}

#[test]
fn feedback_view_en_json() {
    let mut c = new_contract();
    rate(&mut c, PAYER, &Pedido::default(), &released_json()).unwrap();
    let v = near_sdk::serde_json::to_value(c.get_feedback(0).unwrap()).unwrap();
    assert_eq!(v["ratee_role"], "receiver");
    assert_eq!(v["released"], "1000000");
    assert_eq!(v["revoked"], false);
    assert_eq!(v["revoked_at"], near_sdk::serde_json::Value::Null);
    let s = near_sdk::serde_json::to_value(c.get_summary(acc(WORKER))).unwrap();
    assert_eq!(s["score_sum"], "90");
    assert_eq!(s["average_x100"], 9000);
    assert_eq!(s["last_at"], (NOW + 1_000_000_000).to_string());
}
