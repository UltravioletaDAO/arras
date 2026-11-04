//! `append_response` y `get_response_count`.

use super::common::*;
use crate::*;
use near_sdk::serde_json::json;

const RURI: &str = "ipfs://bafyrespuesta";

fn respond(c: &mut ReputationRegistry, who: &str, client: &str, idx: &str) {
    ctx(who, NEAR);
    c.append_response(
        "7".into(),
        acc(client),
        idx.into(),
        RURI.into(),
        Some(HASH.into()),
    );
}

#[test]
fn cualquiera_responde_y_se_cuenta_por_respondedor() {
    let mut c = new_contract();
    rate(&mut c, CLIENT, &Pedido::default());
    respond(&mut c, OWNER, CLIENT, "1");
    assert_eq!(
        events(),
        vec![(
            "response_appended".into(),
            json!({"agent_id": "7", "client_address": CLIENT, "feedback_index": "1", "responder": OWNER,
                   "response_uri": RURI, "response_hash": HASH})
        )]
    );
    respond(&mut c, OWNER, CLIENT, "1");
    respond(&mut c, OTHER, CLIENT, "1");
    respond(&mut c, CLIENT, CLIENT, "1");
    assert_eq!(
        c.get_response_count("7".into(), Some(acc(CLIENT)), Some("1".into()), None)
            .0,
        4
    );
    assert_eq!(
        c.get_response_count(
            "7".into(),
            Some(acc(CLIENT)),
            Some("1".into()),
            Some(vec![acc(OWNER)])
        )
        .0,
        2
    );
    // Un respondedor repetido en el filtro cuenta dos veces (como EVM).
    assert_eq!(
        c.get_response_count(
            "7".into(),
            Some(acc(CLIENT)),
            Some("1".into()),
            Some(vec![acc(OTHER), acc(OTHER)])
        )
        .0,
        2
    );
    assert_eq!(
        c.get_response_count(
            "7".into(),
            Some(acc(CLIENT)),
            Some("1".into()),
            Some(vec![])
        )
        .0,
        4
    );
}

#[test]
fn get_response_count_por_cliente_y_por_agente() {
    let mut c = new_contract();
    rate(&mut c, CLIENT, &Pedido::default());
    rate(&mut c, CLIENT, &Pedido::default());
    rate(&mut c, CLIENT2, &Pedido::default());
    respond(&mut c, OWNER, CLIENT, "1");
    respond(&mut c, OWNER, CLIENT, "2");
    respond(&mut c, OTHER, CLIENT, "2");
    respond(&mut c, OWNER, CLIENT2, "1");
    assert_eq!(
        c.get_response_count("7".into(), Some(acc(CLIENT)), None, None)
            .0,
        3
    );
    assert_eq!(
        c.get_response_count("7".into(), Some(acc(CLIENT)), Some("0".into()), None)
            .0,
        3
    );
    assert_eq!(c.get_response_count("7".into(), None, None, None).0, 4);
    assert_eq!(
        c.get_response_count("7".into(), None, Some("2".into()), Some(vec![acc(OWNER)]))
            .0,
        3
    );
    assert_eq!(c.get_response_count("8".into(), None, None, None).0, 0);
}

#[test]
fn tambien_a_una_revocada() {
    let mut c = new_contract();
    rate(&mut c, CLIENT, &Pedido::default());
    ctx(CLIENT, 1);
    c.revoke_feedback("7".into(), "1".into());
    respond(&mut c, OWNER, CLIENT, "1");
    assert_eq!(c.get_response_count("7".into(), None, None, None).0, 1);
}

#[test]
fn el_respondedor_nuevo_paga_y_el_repetido_no() {
    let mut c = new_contract();
    rate(&mut c, CLIENT, &Pedido::default());
    ctx(OWNER, NEAR);
    c.flush_all();
    let antes = env::storage_usage();
    c.append_response("7".into(), acc(CLIENT), "1".into(), RURI.into(), None);
    c.flush_all();
    let bytes = env::storage_usage() - antes;
    assert!(bytes > 0);
    assert_eq!(
        transfers(),
        vec![(OWNER.to_string(), NEAR - bytes as u128 * BYTE)]
    );
    ctx(OWNER, 1);
    c.append_response("7".into(), acc(CLIENT), "1".into(), RURI.into(), None);
    assert_eq!(transfers(), vec![(OWNER.to_string(), 1)]);
    assert_eq!(events()[0].1["response_hash"], json!(null));
}

#[test]
#[should_panic(expected = "ERC8004_REP_E09")]
fn respondedor_nuevo_sin_deposito_suficiente_es_e09() {
    let mut c = new_contract();
    rate(&mut c, CLIENT, &Pedido::default());
    ctx(OWNER, 1);
    c.append_response("7".into(), acc(CLIENT), "1".into(), RURI.into(), None);
}

#[test]
#[should_panic(expected = "ERC8004_REP_E15")]
fn sin_yocto_es_e15() {
    let mut c = new_contract();
    rate(&mut c, CLIENT, &Pedido::default());
    ctx(OWNER, 0);
    c.append_response("7".into(), acc(CLIENT), "1".into(), RURI.into(), None);
}

#[test]
#[should_panic(expected = "ERC8004_REP_E12")]
fn indice_cero_es_e12() {
    let mut c = new_contract();
    rate(&mut c, CLIENT, &Pedido::default());
    respond(&mut c, OWNER, CLIENT, "0");
}

#[test]
#[should_panic(expected = "ERC8004_REP_E16")]
fn uri_vacia_es_e16() {
    let mut c = new_contract();
    rate(&mut c, CLIENT, &Pedido::default());
    ctx(OWNER, NEAR);
    c.append_response("7".into(), acc(CLIENT), "1".into(), "".into(), None);
}

#[test]
#[should_panic(expected = "ERC8004_REP_E16")]
fn uri_invalida_es_e16() {
    let mut c = new_contract();
    rate(&mut c, CLIENT, &Pedido::default());
    ctx(OWNER, NEAR);
    c.append_response(
        "7".into(),
        acc(CLIENT),
        "1".into(),
        "ipfs://a b".into(),
        None,
    );
}

#[test]
#[should_panic(expected = "ERC8004_REP_E16")]
fn uri_larga_es_e16() {
    let mut c = new_contract();
    rate(&mut c, CLIENT, &Pedido::default());
    ctx(OWNER, NEAR);
    c.append_response(
        "7".into(),
        acc(CLIENT),
        "1".into(),
        format!("a:{}", "b".repeat(MAX_RESPONSE_URI_LEN - 1)),
        None,
    );
}

#[test]
#[should_panic(expected = "ERC8004_REP_E13")]
fn indice_de_mas_es_e13() {
    let mut c = new_contract();
    rate(&mut c, CLIENT, &Pedido::default());
    respond(&mut c, OWNER, CLIENT, "2");
}

#[test]
#[should_panic(expected = "ERC8004_REP_E13")]
fn sin_calificaciones_es_e13() {
    let mut c = new_contract();
    respond(&mut c, OWNER, CLIENT, "1");
}

#[test]
#[should_panic(expected = "ERC8004_REP_E08")]
fn hash_invalido_es_e08() {
    let mut c = new_contract();
    rate(&mut c, CLIENT, &Pedido::default());
    ctx(OWNER, NEAR);
    c.append_response(
        "7".into(),
        acc(CLIENT),
        "1".into(),
        RURI.into(),
        Some(format!("0x{}", &HASH[2..])),
    );
}

#[test]
#[should_panic(expected = "ERC8004_REP_E11")]
fn indice_no_canonico_es_e11() {
    let mut c = new_contract();
    rate(&mut c, CLIENT, &Pedido::default());
    respond(&mut c, OWNER, CLIENT, "+1");
}
