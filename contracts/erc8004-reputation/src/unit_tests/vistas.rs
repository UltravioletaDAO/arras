//! `get_summary` (el algoritmo de EVM), `read_feedback`, `read_all_feedback`, `get_clients`, `get_last_index` y
//! `get_config`.

use super::common::*;
use crate::*;
use near_sdk::serde_json::{self, json};

fn summary(
    c: &ReputationRegistry,
    clients: &[&str],
    t1: Option<&str>,
    t2: Option<&str>,
) -> (u64, String, u8) {
    let s = c.get_summary(
        "7".into(),
        clients.iter().map(|x| acc(x)).collect(),
        t1.map(Into::into),
        t2.map(Into::into),
    );
    (s.count.0, s.summary_value, s.summary_value_decimals)
}

#[test]
fn get_summary_como_evm() {
    let mut c = new_contract();
    rate(&mut c, CLIENT, &pedido("7", "9977", 2, "uptime", "week"));
    rate(&mut c, CLIENT, &pedido("7", "9810", 2, "uptime", "day"));
    rate(&mut c, CLIENT2, &pedido("7", "87", 0, "starred", ""));
    rate(
        &mut c,
        CLIENT2,
        &pedido("7", "-32", 1, "tradingYield", "week"),
    );
    assert_eq!(summary(&c, &[CLIENT], None, None), (2, "9893".into(), 2));
    assert_eq!(
        summary(&c, &[CLIENT], Some("uptime"), Some("day")),
        (1, "9810".into(), 2)
    );
    assert_eq!(
        summary(&c, &[CLIENT], Some(""), Some("")),
        (2, "9893".into(), 2)
    );
    assert_eq!(
        summary(&c, &[CLIENT2], Some("starred"), None),
        (1, "87".into(), 0)
    );
    assert_eq!(
        summary(&c, &[CLIENT2], None, Some("week")),
        (1, "-32".into(), 1)
    );
    assert_eq!(summary(&c, &[OTHER], None, None), (0, "0".into(), 0));
    // Un cliente repetido cuenta dos veces, como en EVM.
    assert_eq!(summary(&c, &[CLIENT2, CLIENT2], Some("starred"), None).0, 2);
    // Los cuatro: moda 2 decimales (2 de 4).
    let s = summary(&c, &[CLIENT, CLIENT2], None, None);
    assert_eq!((s.0, s.2), (4, 2));
    // (99,77 + 98,10 + 87 - 3,2) / 4 = 70,4175 -> 7041 con 2 decimales.
    assert_eq!(s.1, "7041");
    // Las de otro agente no cuentan.
    rate(&mut c, CLIENT, &pedido("8", "1", 0, "", ""));
    assert_eq!(summary(&c, &[CLIENT], None, None).0, 2);
}

#[test]
#[should_panic(expected = "ERC8004_REP_E30: clientAddresses required")]
fn get_summary_sin_clientes_es_e30() {
    let c = new_contract();
    c.get_summary("7".into(), vec![], None, None);
}

#[test]
#[should_panic(expected = "ERC8004_REP_E01")]
fn get_summary_con_id_invalido_es_e01() {
    let c = new_contract();
    c.get_summary("x".into(), vec![acc(CLIENT)], None, None);
}

#[test]
fn read_feedback_y_null() {
    let mut c = new_contract();
    rate(&mut c, CLIENT, &pedido("7", "-5", 3, "a", "b"));
    assert_eq!(
        serde_json::to_value(c.read_feedback("7".into(), acc(CLIENT), "1".into())).unwrap(),
        json!({"value": "-5", "value_decimals": 3, "tag1": "a", "tag2": "b", "is_revoked": false})
    );
    for (id, client, idx) in [
        ("7", CLIENT, "0"),
        ("7", CLIENT, "2"),
        ("7", CLIENT, "01"),
        ("8", CLIENT, "1"),
        ("7", CLIENT2, "1"),
        ("x", CLIENT, "1"),
    ] {
        assert_eq!(
            c.read_feedback(id.into(), acc(client), idx.into()),
            None,
            "{id} {client} {idx}"
        );
    }
}

#[test]
fn read_all_feedback_como_evm() {
    let mut c = new_contract();
    rate(&mut c, CLIENT, &pedido("7", "1", 0, "a", "x"));
    rate(&mut c, CLIENT2, &pedido("7", "2", 1, "b", "x"));
    rate(&mut c, CLIENT, &pedido("7", "3", 2, "a", "y"));
    ctx(CLIENT, 1);
    c.revoke_feedback("7".into(), "2".into());
    let all = c.read_all_feedback("7".into(), None, None, None, None);
    assert_eq!(
        serde_json::to_value(&all).unwrap(),
        json!({"clients": [CLIENT, CLIENT2], "feedback_indexes": ["1", "1"], "values": ["1", "2"],
               "value_decimals": [0, 1], "tag1s": ["a", "b"], "tag2s": ["x", "x"], "revoked_statuses": [false, false]})
    );
    let con = c.read_all_feedback("7".into(), Some(vec![]), None, None, Some(true));
    assert_eq!(con.feedback_indexes, ["1", "2", "1"]);
    assert_eq!(con.revoked_statuses, [false, true, false]);
    let filtro = c.read_all_feedback(
        "7".into(),
        Some(vec![acc(CLIENT)]),
        Some("a".into()),
        Some("y".into()),
        Some(true),
    );
    assert_eq!(filtro.values, ["3"]);
    assert_eq!(
        c.read_all_feedback("7".into(), Some(vec![acc(OTHER)]), None, None, None),
        AllFeedbackView::default()
    );
}

#[test]
fn get_clients_y_paginas() {
    let mut c = new_contract();
    for i in 0..5 {
        rate(&mut c, &format!("c{i}.testnet"), &Pedido::default());
    }
    rate(&mut c, "c0.testnet", &Pedido::default());
    let ids = |v: Vec<AccountId>| v.into_iter().map(|a| a.to_string()).collect::<Vec<_>>();
    assert_eq!(
        ids(c.get_clients("7".into(), None, None)),
        [
            "c0.testnet",
            "c1.testnet",
            "c2.testnet",
            "c3.testnet",
            "c4.testnet"
        ]
    );
    assert_eq!(
        ids(c.get_clients("7".into(), Some(1), Some(2))),
        ["c1.testnet", "c2.testnet"]
    );
    assert_eq!(
        ids(c.get_clients("7".into(), Some(4), None)),
        ["c4.testnet"]
    );
    assert!(c.get_clients("7".into(), Some(9), Some(1)).is_empty());
    assert!(c.get_clients("8".into(), None, None).is_empty());
}

#[test]
fn get_clients_pagina_hasta_100() {
    let mut c = new_contract();
    for i in 0..101 {
        rate(&mut c, &format!("c{i}.testnet"), &Pedido::default());
    }
    assert_eq!(c.get_clients("7".into(), None, None).len(), 101);
    assert_eq!(c.get_clients("7".into(), Some(0), Some(500)).len(), 100);
}

#[test]
fn config() {
    let c = new_contract();
    let v = serde_json::to_value(c.get_config()).unwrap();
    assert_eq!(v["contract"], "erc8004-reputation");
    assert_eq!(v["erc8004_version"], "2.0.0");
    assert_eq!(v["identity_registry"], IDENTITY);
    assert_eq!(
        v["feedback_deposit"],
        (FEEDBACK_MAX_BYTES as u128 * BYTE).to_string()
    );
    assert_eq!(
        v["min_gas_give_feedback"],
        MIN_GAS_GIVE_FEEDBACK.as_gas().to_string()
    );
    #[cfg(feature = "feedback-proofs")]
    {
        assert_eq!(
            v["feedback_proof_deposit"],
            (FEEDBACK_PROOF_MAX_BYTES as u128 * BYTE).to_string()
        );
        assert_eq!(
            v["min_gas_give_feedback_with_proof"],
            MIN_GAS_GIVE_FEEDBACK_WITH_PROOF.as_gas().to_string()
        );
    }
    #[cfg(not(feature = "feedback-proofs"))]
    assert_eq!(v["feedback_proof_deposit"], json!(null));
    assert_eq!(c.get_identity_registry(), acc(IDENTITY));
    assert_eq!(c.get_version(), "2.0.0");
}

#[test]
fn get_summary_con_signos_mezclados() {
    // Refutador, P1: calificaciones de 0..100 positivas y negativas mezcladas.
    let mut c = new_contract();
    rate(&mut c, CLIENT, &pedido("7", "19", 0, "starred", ""));
    rate(&mut c, CLIENT2, &pedido("7", "-1", 0, "starred", ""));
    assert_eq!(
        summary(&c, &[CLIENT, CLIENT2], None, None),
        (2, "9".into(), 0)
    );
    rate(&mut c, CLIENT2, &pedido("7", "-100", 0, "starred", ""));
    // (19 - 1 - 100) / 3 = -27,33 -> -27
    assert_eq!(
        summary(&c, &[CLIENT, CLIENT2], None, None),
        (3, "-27".into(), 0)
    );
}
