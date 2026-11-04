//! Las vistas: NEP-171/177/178/181, `check_agent` y las de ERC-8004.

use super::common::*;
use crate::*;
use near_sdk::json_types::U128;
use near_sdk::serde_json::{self, json};

#[test]
fn nft_token_en_json() {
    let mut c = new_contract();
    reg_uri(&mut c, ALICE, URI);
    approve(&mut c, ALICE, "0", OP);
    let t = serde_json::to_value(c.nft_token("0".into()).unwrap()).unwrap();
    assert_eq!(
        t,
        json!({
            "token_id": "0",
            "owner_id": ALICE,
            "metadata": {
                "title": null, "description": null, "media": null, "media_hash": null, "copies": null,
                "issued_at": null, "expires_at": null, "starts_at": null, "updated_at": null, "extra": null,
                "reference": URI, "reference_hash": null
            },
            "approved_account_ids": {OP: 0}
        })
    );
    assert!(c.nft_token("1".into()).is_none());
    assert!(c.nft_token("00".into()).is_none());
    assert!(c.nft_token("x".into()).is_none());
}

#[test]
fn nft_metadata_del_contrato() {
    let c = new_contract();
    assert_eq!(
        serde_json::to_value(c.nft_metadata()).unwrap(),
        json!({"spec": "nft-1.0.0", "name": "AgentIdentity", "symbol": "AGENT", "icon": null, "base_uri": null,
               "reference": null, "reference_hash": null})
    );
}

#[test]
fn nft_tokens_pagina_en_orden_de_id() {
    let mut c = new_contract();
    for w in [ALICE, BOB, ALICE, CAROL, BOB] {
        reg(&mut c, w);
    }
    let ids = |v: Vec<JsonToken>| v.into_iter().map(|t| t.token_id).collect::<Vec<_>>();
    assert_eq!(ids(c.nft_tokens(None, None)), ["0", "1", "2", "3", "4"]);
    assert_eq!(ids(c.nft_tokens(Some(U128(2)), Some(2))), ["2", "3"]);
    assert_eq!(ids(c.nft_tokens(Some(U128(4)), Some(10))), ["4"]);
    assert!(c.nft_tokens(Some(U128(5)), None).is_empty());
    assert!(
        c.nft_tokens(Some(U128(u128::MAX)), Some(u64::MAX))
            .is_empty()
    );
    assert_eq!(c.nft_total_supply(), U128(5));
}

#[test]
fn limit_tope_100() {
    let mut c = new_contract();
    for _ in 0..(MAX_LIST_LIMIT + 1) {
        reg(&mut c, ALICE);
    }
    assert_eq!(
        c.nft_tokens(None, Some(1_000)).len(),
        MAX_LIST_LIMIT as usize
    );
    assert_eq!(c.nft_tokens(None, None).len(), MAX_LIST_LIMIT as usize);
    assert_eq!(
        c.nft_tokens_for_owner(acc(ALICE), None, Some(1_000)).len(),
        MAX_LIST_LIMIT as usize
    );
    assert_eq!(
        c.nft_supply_for_owner(acc(ALICE)),
        U128(u128::from(MAX_LIST_LIMIT + 1))
    );
}

#[test]
fn el_indice_por_dueno_sigue_bien_despues_de_transferir() {
    let mut c = new_contract();
    for _ in 0..4 {
        reg(&mut c, ALICE);
    }
    let ids = |c: &IdentityRegistry, w: &str| {
        let mut v: Vec<String> = c
            .nft_tokens_for_owner(acc(w), None, None)
            .into_iter()
            .map(|t| t.token_id)
            .collect();
        v.sort();
        v
    };
    transfer(&mut c, ALICE, BOB, "0");
    assert_eq!(ids(&c, ALICE), ["1", "2", "3"]);
    assert_eq!(ids(&c, BOB), ["0"]);
    transfer(&mut c, ALICE, BOB, "2");
    transfer(&mut c, ALICE, BOB, "3");
    assert_eq!(ids(&c, ALICE), ["1"]);
    assert_eq!(ids(&c, BOB), ["0", "2", "3"]);
    // Cada token sabe su posicion: sacar del medio y del final.
    transfer(&mut c, BOB, CAROL, "2");
    transfer(&mut c, BOB, CAROL, "0");
    transfer(&mut c, ALICE, CAROL, "1");
    assert_eq!(ids(&c, ALICE), Vec::<String>::new());
    assert_eq!(ids(&c, BOB), ["3"]);
    assert_eq!(ids(&c, CAROL), ["0", "1", "2"]);
    transfer(&mut c, CAROL, BOB, "1");
    assert_eq!(ids(&c, CAROL), ["0", "2"]);
    assert_eq!(ids(&c, BOB), ["1", "3"]);
    for (w, n) in [(ALICE, 0), (BOB, 2), (CAROL, 2)] {
        assert_eq!(c.nft_supply_for_owner(acc(w)), U128(n));
    }
    let pagina: Vec<String> = c
        .nft_tokens_for_owner(acc(CAROL), Some(U128(1)), Some(5))
        .into_iter()
        .map(|t| t.token_id)
        .collect();
    assert_eq!(pagina.len(), 1);
}

#[test]
fn check_agent_para_las_llamadas_cruzadas() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    approve(&mut c, ALICE, "0", OP);
    let v = |c: &IdentityRegistry, id: &str, who: &str| {
        serde_json::to_value(c.check_agent(id.into(), acc(who))).unwrap()
    };
    assert_eq!(
        v(&c, "0", ALICE),
        json!({"owner_id": ALICE, "agent_wallet": ALICE, "authorized": true})
    );
    assert_eq!(
        v(&c, "0", OP),
        json!({"owner_id": ALICE, "agent_wallet": ALICE, "authorized": true})
    );
    assert_eq!(
        v(&c, "0", BOB),
        json!({"owner_id": ALICE, "agent_wallet": ALICE, "authorized": false})
    );
    assert_eq!(v(&c, "1", ALICE), json!(null));
    assert_eq!(v(&c, "00", ALICE), json!(null));
    assert_eq!(v(&c, "-0", ALICE), json!(null));
    transfer(&mut c, ALICE, BOB, "0");
    assert_eq!(
        v(&c, "0", ALICE),
        json!({"owner_id": BOB, "agent_wallet": null, "authorized": false})
    );
    assert_eq!(
        v(&c, "0", OP),
        json!({"owner_id": BOB, "agent_wallet": null, "authorized": false})
    );
}

#[test]
fn is_authorized_or_owner_como_evm() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    approve(&mut c, ALICE, "0", OP);
    assert!(c.is_authorized_or_owner(acc(ALICE), "0".into()));
    assert!(c.is_authorized_or_owner(acc(OP), "0".into()));
    assert!(!c.is_authorized_or_owner(acc(BOB), "0".into()));
}

#[test]
#[should_panic(expected = "ERC8004_ID_E11")]
fn is_authorized_or_owner_de_inexistente_entra_en_panico() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    c.is_authorized_or_owner(acc(ALICE), "1".into());
}

#[test]
fn vistas_de_un_agente_inexistente_son_null() {
    let c = new_contract();
    assert_eq!(c.get_metadata("0".into(), "agentWallet".into()), None);
    assert_eq!(c.get_metadata("0".into(), "k".into()), None);
    assert_eq!(c.get_agent_wallet("0".into()), None);
    assert_eq!(c.get_pending_agent_wallet("0".into()), None);
    assert_eq!(c.token_uri("0".into()), None);
    assert!(c.nft_tokens_for_owner(acc(ALICE), None, None).is_empty());
    assert_eq!(c.nft_supply_for_owner(acc(ALICE)), U128(0));
}

#[test]
fn get_config() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    let v = serde_json::to_value(c.get_config()).unwrap();
    assert_eq!(v["contract"], "erc8004-identity");
    assert_eq!(v["erc8004_version"], "2.0.0");
    assert_eq!(v["events_standard"], "erc8004");
    assert_eq!(v["events_version"], "1.0.0");
    assert_eq!(v["total_agents"], "1");
    assert_eq!(
        v["register_deposit"],
        (REGISTER_MAX_BYTES as u128 * BYTE).to_string()
    );
    assert_eq!(v["token_reserve_bytes"], TOKEN_RESERVE_BYTES);
    assert_eq!(v["storage_byte_cost"], BYTE.to_string());
    assert_eq!(
        v["min_gas_nft_transfer_call"],
        MIN_GAS_NFT_TRANSFER_CALL.as_gas().to_string()
    );
    assert_eq!(
        v["limits"],
        json!({"max_agent_uri_len": 8192, "max_metadata_key_len": 64, "max_metadata_value_len": 1024,
               "max_register_metadata": 4, "max_approvals": 8, "max_memo_len": 256,
               "max_deadline_delay_ns": "300000000000", "max_list_limit": 100})
    );
}
