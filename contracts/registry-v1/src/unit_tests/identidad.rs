//! Identidad: `register` y `update` (E01..E06), storage cobrado y devuelto, eventos y vistas.

use super::common::*;
use crate::*;

const UN_NEAR: u128 = 1_000_000_000_000_000_000_000_000;

/// `register` con `dep` y devuelve (vista, bytes que crecio el storage).
fn register(
    c: &mut Registry,
    who: &str,
    dep: u128,
    uri: &str,
    meta: Option<&str>,
) -> (AgentView, u64) {
    ctx(who, dep);
    c.flush_all();
    let antes = env::storage_usage();
    let v = c.register(uri.into(), meta.map(String::from));
    c.flush_all();
    (v, env::storage_usage() - antes)
}

#[test]
fn register_cobra_lo_medido_y_devuelve_el_resto() {
    let mut c = new_contract();
    let (v, bytes) = register(&mut c, PAYER, UN_NEAR, URI, Some(r#"{"nombre":"Agente"}"#));
    let costo = u128::from(bytes) * 10u128.pow(19);
    assert!(bytes > 0);
    assert_eq!(transfers(), vec![(PAYER.to_string(), UN_NEAR - costo)]);
    assert_eq!(
        v,
        AgentView {
            account_id: acc(PAYER),
            agent_uri: URI.into(),
            metadata: Some(r#"{"nombre":"Agente"}"#.into()),
            registered_at: U64(NOW),
            updated_at: U64(NOW),
        }
    );
    assert_eq!(c.get_agent(acc(PAYER)), Some(v));
    let ev = events("agent_registered");
    assert_eq!(ev.len(), 1);
    assert_eq!(ev[0]["version"], "1.0.0");
    assert_eq!(
        ev[0]["data"],
        near_sdk::serde_json::json!([{
            "account_id": PAYER,
            "agent_uri": URI,
            "metadata": "{\"nombre\":\"Agente\"}",
            "at": NOW.to_string(),
        }])
    );
    assert_eq!(c.get_config().agents, 1);
}

#[test]
fn register_con_el_deposito_justo_no_devuelve_nada() {
    // El costo de la misma ficha medido con otra cuenta del mismo largo (`worker.testnet` y `agente.testnet`).
    assert_eq!(WORKER.len(), PAYER.len());
    let mut c = new_contract();
    let (_, bytes) = register(&mut c, WORKER, UN_NEAR, URI, None);
    let costo = u128::from(bytes) * 10u128.pow(19);
    register(&mut c, PAYER, costo, URI, None);
    assert!(transfers().is_empty(), "{:?}", transfers());
    assert!(c.get_agent(acc(PAYER)).is_some());
}

#[test]
#[should_panic(expected = "ARRAS_REG_E05: deposito insuficiente para el storage: se necesitan")]
fn register_con_un_yocto_menos_es_e05() {
    let mut c = new_contract();
    let (_, bytes) = register(&mut c, WORKER, UN_NEAR, URI, None);
    let costo = u128::from(bytes) * 10u128.pow(19);
    register(&mut c, PAYER, costo - 1, URI, None);
}

#[test]
#[should_panic(expected = "ARRAS_REG_E05")]
fn register_sin_deposito_es_e05() {
    let mut c = new_contract();
    register(&mut c, PAYER, 0, URI, None);
}

#[test]
#[should_panic(expected = "ARRAS_REG_E03")]
fn register_dos_veces_es_e03() {
    let mut c = new_contract();
    register(&mut c, PAYER, UN_NEAR, URI, None);
    register(&mut c, PAYER, UN_NEAR, "ipfs://otra", None);
}

#[test]
#[should_panic(expected = "ARRAS_REG_E01")]
fn register_uri_vacia_es_e01() {
    let mut c = new_contract();
    register(&mut c, PAYER, UN_NEAR, "", None);
}

#[test]
#[should_panic(expected = "ARRAS_REG_E01")]
fn register_uri_sin_esquema_es_e01() {
    let mut c = new_contract();
    register(&mut c, PAYER, UN_NEAR, "agente.example/card.json", None);
}

#[test]
#[should_panic(expected = "ARRAS_REG_E01")]
fn register_uri_larga_es_e01() {
    let mut c = new_contract();
    let larga = format!("https://{}", "a".repeat(505));
    register(&mut c, PAYER, UN_NEAR, &larga, None);
}

#[test]
#[should_panic(expected = "ARRAS_REG_E02")]
fn register_metadata_vacia_es_e02() {
    let mut c = new_contract();
    register(&mut c, PAYER, UN_NEAR, URI, Some(""));
}

#[test]
#[should_panic(expected = "ARRAS_REG_E02")]
fn register_metadata_larga_es_e02() {
    let mut c = new_contract();
    register(&mut c, PAYER, UN_NEAR, URI, Some(&"m".repeat(1025)));
}

#[test]
fn register_al_tope_de_largo_pasa() {
    let mut c = new_contract();
    let uri = format!("https://{}", "a".repeat(504));
    assert_eq!(uri.len(), 512);
    let (v, _) = register(&mut c, PAYER, UN_NEAR, &uri, Some(&"m".repeat(1024)));
    assert_eq!(v.agent_uri, uri);
}

/// `update` con `dep`; devuelve (vista, crecimiento con signo).
fn update(
    c: &mut Registry,
    who: &str,
    dep: u128,
    uri: &str,
    meta: Option<&str>,
) -> (AgentView, i64) {
    ctx_full(who, dep, NOW + 5, HEIGHT, Gas::from_tgas(300));
    c.flush_all();
    let antes = env::storage_usage() as i64;
    let v = c.update(uri.into(), meta.map(String::from));
    c.flush_all();
    (v, env::storage_usage() as i64 - antes)
}

#[test]
fn update_que_crece_cobra_la_diferencia() {
    let mut c = new_contract();
    register(&mut c, PAYER, UN_NEAR, URI, None);
    let (v, delta) = update(&mut c, PAYER, UN_NEAR, URI, Some(&"m".repeat(100)));
    assert!(delta > 0);
    let costo = delta as u128 * 10u128.pow(19);
    assert_eq!(transfers(), vec![(PAYER.to_string(), UN_NEAR - costo)]);
    assert_eq!(v.registered_at, U64(NOW));
    assert_eq!(v.updated_at, U64(NOW + 5));
    assert_eq!(v.metadata.as_deref(), Some("m".repeat(100).as_str()));
    let ev = events("agent_updated");
    assert_eq!(ev.len(), 1);
    assert_eq!(ev[0]["data"][0]["at"], (NOW + 5).to_string());
    assert!(events("agent_registered").is_empty());
}

#[test]
fn update_que_achica_devuelve_lo_liberado_y_el_deposito() {
    let mut c = new_contract();
    register(&mut c, PAYER, UN_NEAR, URI, Some(&"m".repeat(500)));
    let (v, delta) = update(&mut c, PAYER, 1, URI, None);
    assert!(delta < 0);
    let liberado = (-delta) as u128 * 10u128.pow(19);
    assert_eq!(transfers(), vec![(PAYER.to_string(), 1 + liberado)]);
    assert_eq!(v.metadata, None);
    assert_eq!(c.get_agent(acc(PAYER)).unwrap().metadata, None);
}

#[test]
fn update_del_mismo_largo_devuelve_el_deposito() {
    let mut c = new_contract();
    register(&mut c, PAYER, UN_NEAR, "ipfs://aaaa", None);
    let (v, delta) = update(&mut c, PAYER, 7, "ipfs://bbbb", None);
    assert_eq!(delta, 0);
    assert_eq!(transfers(), vec![(PAYER.to_string(), 7)]);
    assert_eq!(v.agent_uri, "ipfs://bbbb");
}

#[test]
#[should_panic(expected = "ARRAS_REG_E05")]
fn update_que_crece_sin_deposito_suficiente_es_e05() {
    let mut c = new_contract();
    register(&mut c, PAYER, UN_NEAR, URI, None);
    update(&mut c, PAYER, 1, URI, Some(&"m".repeat(100)));
}

#[test]
#[should_panic(expected = "ARRAS_REG_E06")]
fn update_sin_yocto_es_e06() {
    let mut c = new_contract();
    register(&mut c, PAYER, UN_NEAR, URI, Some(&"m".repeat(500)));
    update(&mut c, PAYER, 0, URI, None);
}

#[test]
#[should_panic(expected = "ARRAS_REG_E04")]
fn update_sin_registro_es_e04() {
    let mut c = new_contract();
    update(&mut c, PAYER, UN_NEAR, URI, None);
}

#[test]
#[should_panic(expected = "ARRAS_REG_E01")]
fn update_con_uri_invalida_es_e01() {
    let mut c = new_contract();
    register(&mut c, PAYER, UN_NEAR, URI, None);
    update(&mut c, PAYER, UN_NEAR, "https://con espacio", None);
}

#[test]
#[should_panic(expected = "ARRAS_REG_E02")]
fn update_con_metadata_invalida_es_e02() {
    let mut c = new_contract();
    register(&mut c, PAYER, UN_NEAR, URI, None);
    update(&mut c, PAYER, UN_NEAR, URI, Some("a\nb"));
}

#[test]
fn update_solo_toca_la_cuenta_que_llama() {
    let mut c = new_contract();
    register(&mut c, PAYER, UN_NEAR, URI, None);
    register(&mut c, WORKER, UN_NEAR, "ipfs://worker", None);
    update(&mut c, PAYER, UN_NEAR, "ipfs://nueva", None);
    assert_eq!(c.get_agent(acc(PAYER)).unwrap().agent_uri, "ipfs://nueva");
    assert_eq!(c.get_agent(acc(WORKER)).unwrap().agent_uri, "ipfs://worker");
}

#[test]
fn list_agents_pagina_en_orden_de_alta() {
    let mut c = new_contract();
    let nombres: Vec<String> = (0..55).map(|i| format!("agente{i:02}.testnet")).collect();
    for n in &nombres {
        register(&mut c, n, UN_NEAR, URI, None);
    }
    assert_eq!(c.get_agent(acc(OTHER)), None);
    let todos = c.list_agents(None, None);
    assert_eq!(todos.len(), 50, "limit por defecto y tope: 50");
    assert_eq!(
        c.list_agents(Some(0), Some(100)).len(),
        50,
        "limit de mas se recorta a 50"
    );
    assert_eq!(todos[0].account_id, acc(&nombres[0]));
    assert_eq!(todos[49].account_id, acc(&nombres[49]));
    let resto = c.list_agents(Some(50), Some(100));
    let ids: Vec<String> = resto.iter().map(|a| a.account_id.to_string()).collect();
    assert_eq!(ids, nombres[50..].to_vec());
    assert_eq!(c.list_agents(Some(3), Some(2)).len(), 2);
    assert_eq!(
        c.list_agents(Some(3), Some(2))[0].account_id,
        acc(&nombres[3])
    );
    assert!(c.list_agents(Some(55), None).is_empty());
    assert!(c.list_agents(Some(u32::MAX), Some(u32::MAX)).is_empty());
    assert!(c.list_agents(Some(0), Some(0)).is_empty());
    assert_eq!(c.get_config().agents, 55);
}
