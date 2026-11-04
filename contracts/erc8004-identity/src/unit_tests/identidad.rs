//! `register`, `set_agent_uri` y `set_metadata`: cada guarda, los eventos y el storage cobrado.

use super::common::*;
use crate::*;
use near_sdk::json_types::Base64VecU8;
use near_sdk::serde_json::json;

fn entry(k: &str, v: &[u8]) -> registry::MetadataEntry {
    registry::MetadataEntry {
        metadata_key: k.into(),
        metadata_value: Base64VecU8(v.to_vec()),
    }
}

#[test]
fn register_sin_argumentos_acuna_el_0_y_pone_agent_wallet() {
    let mut c = new_contract();
    ctx(ALICE, NEAR);
    assert_eq!(c.register(None, None), "0");
    assert_eq!(
        events(),
        vec![
            (
                "nep171".into(),
                "nft_mint".into(),
                json!({"owner_id": ALICE, "token_ids": ["0"]})
            ),
            (
                "erc8004".into(),
                "registered".into(),
                json!({"agent_id": "0", "agent_uri": "", "owner": ALICE})
            ),
            (
                "erc8004".into(),
                "metadata_set".into(),
                json!({"agent_id": "0", "metadata_key": "agentWallet", "metadata_value": b64(ALICE.as_bytes())})
            ),
        ]
    );
    assert_eq!(c.get_agent_wallet("0".into()), Some(acc(ALICE)));
    assert_eq!(c.token_uri("0".into()), Some(String::new()));
    assert_eq!(c.nft_token("0".into()).unwrap().metadata.reference, None);
    assert_eq!(reg(&mut c, BOB), "1");
    assert_eq!(c.nft_total_supply().0, 2);
}

#[test]
fn register_con_uri_y_metadata() {
    let mut c = new_contract();
    ctx(ALICE, NEAR);
    let id = c.register(
        Some(URI.into()),
        Some(vec![
            entry("agentName", b"alice.near"),
            entry("x", b""),
            entry("agentName", b"v2"),
        ]),
    );
    assert_eq!(id, "0");
    let ev = events();
    assert_eq!(
        ev.iter().map(|e| e.1.as_str()).collect::<Vec<_>>(),
        [
            "nft_mint",
            "registered",
            "metadata_set",
            "metadata_set",
            "metadata_set",
            "metadata_set"
        ]
    );
    assert_eq!(ev[1].2["agent_uri"], URI);
    assert_eq!(
        ev[3].2,
        json!({"agent_id": "0", "metadata_key": "agentName", "metadata_value": b64(b"alice.near")})
    );
    assert_eq!(ev[4].2["metadata_value"], "");
    // Una clave repetida: gana la ultima (como el loop de EVM), y salen los dos eventos.
    assert_eq!(
        c.get_metadata("0".into(), "agentName".into()).unwrap().0,
        b"v2"
    );
    assert_eq!(c.get_metadata("0".into(), "x".into()), None);
    assert_eq!(c.token_uri("0".into()), Some(URI.into()));
    assert_eq!(
        c.nft_token("0".into()).unwrap().metadata.reference,
        Some(URI.into())
    );
}

#[test]
fn register_es_del_predecessor_no_del_signer() {
    let mut c = new_contract();
    ctx_full(MARKET, ALICE, NEAR, NOW, Gas::from_tgas(300));
    c.register(None, None);
    assert_eq!(c.nft_token("0".into()).unwrap().owner_id, acc(MARKET));
    assert_eq!(c.get_agent_wallet("0".into()), Some(acc(MARKET)));
}

#[test]
#[should_panic(expected = "ERC8004_ID_E01: agent_uri invalida")]
fn register_uri_invalida_es_e01() {
    let mut c = new_contract();
    ctx(ALICE, NEAR);
    c.register(Some("sin esquema".into()), None);
}

#[test]
fn register_uri_al_tope_vale() {
    let mut c = new_contract();
    ctx(ALICE, NEAR);
    let uri = format!("https://{}", "a".repeat(MAX_AGENT_URI_LEN - 8));
    assert_eq!(uri.len(), MAX_AGENT_URI_LEN);
    c.register(Some(uri.clone()), None);
    assert_eq!(c.token_uri("0".into()), Some(uri));
}

#[test]
#[should_panic(expected = "ERC8004_ID_E01")]
fn register_uri_de_mas_es_e01() {
    let mut c = new_contract();
    ctx(ALICE, NEAR);
    c.register(
        Some(format!("https://{}", "a".repeat(MAX_AGENT_URI_LEN - 7))),
        None,
    );
}

#[test]
#[should_panic(expected = "ERC8004_ID_E03: demasiadas entradas de metadata")]
fn register_con_cinco_entradas_es_e03() {
    let mut c = new_contract();
    ctx(ALICE, NEAR);
    let e: Vec<_> = (0..5).map(|i| entry(&format!("k{i}"), b"v")).collect();
    c.register(None, Some(e));
}

#[test]
fn register_con_cuatro_entradas_vale() {
    let mut c = new_contract();
    ctx(ALICE, NEAR);
    let e: Vec<_> = (0..4).map(|i| entry(&format!("k{i}"), b"v")).collect();
    c.register(None, Some(e));
    assert_eq!(c.get_metadata("0".into(), "k3".into()).unwrap().0, b"v");
}

#[test]
#[should_panic(expected = "ERC8004_ID_E04: reserved key: agentWallet")]
fn register_con_agent_wallet_es_e04() {
    let mut c = new_contract();
    ctx(ALICE, NEAR);
    c.register(None, Some(vec![entry("agentWallet", b"x")]));
}

#[test]
fn agent_wallet_con_otras_mayusculas_no_es_la_reservada() {
    // keccak256("AgentWallet") != keccak256("agentWallet") en EVM: aca tambien es otra clave.
    let mut c = new_contract();
    ctx(ALICE, NEAR);
    c.register(None, Some(vec![entry("AgentWallet", b"x")]));
    assert_eq!(
        c.get_metadata("0".into(), "AgentWallet".into()).unwrap().0,
        b"x"
    );
}

#[test]
#[should_panic(expected = "ERC8004_ID_E02: clave de metadata invalida")]
fn register_con_clave_invalida_es_e02() {
    let mut c = new_contract();
    ctx(ALICE, NEAR);
    c.register(None, Some(vec![entry("con espacio", b"x")]));
}

#[test]
#[should_panic(expected = "ERC8004_ID_E05: valor de metadata demasiado largo")]
fn register_con_valor_largo_es_e05() {
    let mut c = new_contract();
    ctx(ALICE, NEAR);
    c.register(
        None,
        Some(vec![entry("k", &[7u8; MAX_METADATA_VALUE_LEN + 1])]),
    );
}

#[test]
fn register_con_valor_al_tope_vale() {
    let mut c = new_contract();
    ctx(ALICE, NEAR);
    c.register(None, Some(vec![entry("k", &[7u8; MAX_METADATA_VALUE_LEN])]));
    assert_eq!(
        c.get_metadata("0".into(), "k".into()).unwrap().0.len(),
        MAX_METADATA_VALUE_LEN
    );
}

#[test]
#[should_panic(expected = "ERC8004_ID_E07: requiere al menos 1 yocto adjunto")]
fn register_sin_deposito_es_e07() {
    let mut c = new_contract();
    ctx(ALICE, 0);
    c.register(None, None);
}

#[test]
fn register_cobra_lo_medido_mas_la_reserva_y_devuelve_el_resto() {
    let mut c = new_contract();
    ctx(ALICE, NEAR);
    c.flush_all();
    let antes = env::storage_usage();
    c.register(Some(URI.into()), None);
    c.flush_all();
    let bytes = env::storage_usage() - antes;
    let cobrado = (bytes + TOKEN_RESERVE_BYTES) as u128 * BYTE;
    assert_eq!(transfers(), vec![(ALICE.to_string(), NEAR - cobrado)]);
}

#[test]
#[should_panic(expected = "ERC8004_ID_E06: deposito insuficiente para el storage: se necesitan")]
fn register_con_un_yocto_menos_es_e06() {
    // Lo justo para el primer agente de ALICE: token + indice + contador + reserva.
    let uri_bytes = URI.len() as u64;
    let token = erc8004_common::storage::record_bytes(9, TOKEN_FIXED_BYTES + 4 + uri_bytes);
    let need = (token
        + erc8004_common::storage::record_bytes(37, 8)
        + OWNER_COUNT_RECORD_BYTES
        + TOKEN_RESERVE_BYTES) as u128
        * BYTE;
    let mut c = new_contract();
    ctx(ALICE, need);
    c.register(Some(URI.into()), None);
    assert_eq!(transfers(), vec![]);
    ctx(BOB, need - 1);
    c.register(Some(URI.into()), None);
}

#[test]
fn set_agent_uri_del_dueno_y_de_un_aprobado() {
    let mut c = new_contract();
    reg_uri(&mut c, ALICE, URI);
    ctx(ALICE, NEAR);
    c.set_agent_uri("0".into(), "ipfs://bafy".into());
    assert_eq!(
        events(),
        vec![(
            "erc8004".into(),
            "uri_updated".into(),
            json!({"agent_id": "0", "new_uri": "ipfs://bafy", "updated_by": ALICE})
        )]
    );
    approve(&mut c, ALICE, "0", OP);
    ctx(OP, NEAR);
    c.set_agent_uri("0".into(), "".into());
    assert_eq!(events()[0].2["updated_by"], OP);
    assert_eq!(c.token_uri("0".into()), Some(String::new()));
}

#[test]
#[should_panic(expected = "ERC8004_ID_E12: Not authorized")]
fn set_agent_uri_de_otro_es_e12() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    ctx(BOB, NEAR);
    c.set_agent_uri("0".into(), URI.into());
}

#[test]
#[should_panic(expected = "ERC8004_ID_E11: agente inexistente")]
fn set_agent_uri_de_inexistente_es_e11() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    ctx(ALICE, NEAR);
    c.set_agent_uri("1".into(), URI.into());
}

#[test]
#[should_panic(expected = "ERC8004_ID_E10: agent_id invalido")]
fn set_agent_uri_con_id_no_canonico_es_e10() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    ctx(ALICE, NEAR);
    c.set_agent_uri("00".into(), URI.into());
}

#[test]
#[should_panic(expected = "ERC8004_ID_E01")]
fn set_agent_uri_invalida_es_e01() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    ctx(ALICE, NEAR);
    c.set_agent_uri("0".into(), "https://a b".into());
}

#[test]
#[should_panic(expected = "ERC8004_ID_E07")]
fn set_agent_uri_sin_yocto_es_e07() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    ctx(ALICE, 0);
    c.set_agent_uri("0".into(), URI.into());
}

#[test]
#[should_panic(expected = "ERC8004_ID_E06")]
fn set_agent_uri_que_crece_sin_deposito_es_e06() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    ctx(ALICE, 1);
    c.set_agent_uri("0".into(), URI.into());
}

#[test]
fn set_agent_uri_que_crece_cobra_la_diferencia() {
    let mut c = new_contract();
    reg_uri(&mut c, ALICE, "a:b");
    ctx(ALICE, NEAR);
    c.set_agent_uri("0".into(), "a:bcdef".into());
    assert_eq!(transfers(), vec![(ALICE.to_string(), NEAR - 4 * BYTE)]);
}

#[test]
fn lo_liberado_vuelve_al_dueno_y_el_deposito_al_que_llama() {
    let mut c = new_contract();
    reg_uri(&mut c, ALICE, URI);
    approve(&mut c, ALICE, "0", OP);
    ctx(OP, 5);
    c.set_agent_uri("0".into(), "a:b".into());
    let liberado = (URI.len() - 3) as u128 * BYTE;
    assert_eq!(refunded_to(OP), 5);
    assert_eq!(refunded_to(ALICE), liberado);
    // El dueno que achica recibe las dos cosas en una sola transferencia.
    ctx(ALICE, 5);
    c.set_agent_uri("0".into(), "".into());
    assert_eq!(transfers(), vec![(ALICE.to_string(), 5 + 3 * BYTE)]);
}

#[test]
fn set_metadata_pone_borra_y_emite() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    ctx(ALICE, NEAR);
    c.set_metadata(
        "0".into(),
        "agentName".into(),
        Base64VecU8(b"alice".to_vec()),
    );
    assert_eq!(
        events(),
        vec![(
            "erc8004".into(),
            "metadata_set".into(),
            json!({"agent_id": "0", "metadata_key": "agentName", "metadata_value": b64(b"alice")})
        )]
    );
    assert_eq!(
        c.get_metadata("0".into(), "agentName".into()).unwrap().0,
        b"alice"
    );
    approve(&mut c, ALICE, "0", OP);
    ctx(OP, 1);
    c.set_metadata("0".into(), "agentName".into(), Base64VecU8(vec![]));
    assert_eq!(c.get_metadata("0".into(), "agentName".into()), None);
    assert_eq!(events()[0].2["metadata_value"], "");
    // Lo liberado (la entrada entera) vuelve al dueno.
    let entrada = erc8004_common::storage::record_bytes(1 + 8 + 4 + 9, 4 + 5) as u128 * BYTE;
    assert_eq!(refunded_to(ALICE), entrada);
    assert_eq!(refunded_to(OP), 1);
}

#[test]
#[should_panic(expected = "ERC8004_ID_E04")]
fn set_metadata_agent_wallet_es_e04() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    ctx(ALICE, NEAR);
    c.set_metadata("0".into(), "agentWallet".into(), Base64VecU8(b"x".to_vec()));
}

#[test]
#[should_panic(expected = "ERC8004_ID_E12")]
fn set_metadata_de_otro_es_e12() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    ctx(BOB, NEAR);
    c.set_metadata("0".into(), "k".into(), Base64VecU8(b"x".to_vec()));
}

#[test]
#[should_panic(expected = "ERC8004_ID_E02")]
fn set_metadata_clave_vacia_es_e02() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    ctx(ALICE, NEAR);
    c.set_metadata("0".into(), "".into(), Base64VecU8(b"x".to_vec()));
}

#[test]
#[should_panic(expected = "ERC8004_ID_E05")]
fn set_metadata_valor_largo_es_e05() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    ctx(ALICE, NEAR);
    c.set_metadata(
        "0".into(),
        "k".into(),
        Base64VecU8(vec![1; MAX_METADATA_VALUE_LEN + 1]),
    );
}

#[test]
#[should_panic(expected = "ERC8004_ID_E07")]
fn set_metadata_sin_yocto_es_e07() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    ctx(ALICE, 0);
    c.set_metadata("0".into(), "k".into(), Base64VecU8(vec![]));
}

#[test]
#[should_panic(expected = "ERC8004_ID_E06")]
fn set_metadata_que_crece_sin_deposito_es_e06() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    ctx(ALICE, 1);
    c.set_metadata("0".into(), "k".into(), Base64VecU8(vec![1]));
}

#[test]
fn init_es_privado_y_empieza_vacio() {
    let c = new_contract();
    assert_eq!(c.nft_total_supply().0, 0);
    assert_eq!(c.get_version(), "2.0.0");
}

// `#[private]` lo pone el wrapper de la wasm, no el metodo de Rust: el `new` ajeno y el `nft_resolve_transfer` ajeno se
// prueban en el sandbox (`tests/workspaces.rs`).
