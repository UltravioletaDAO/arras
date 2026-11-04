//! NEP-171 (transferencias y `nft_resolve_transfer`) y NEP-178 (aprobaciones).

use super::common::*;
use crate::*;
use near_sdk::PromiseResult;
use near_sdk::json_types::U128;
use near_sdk::serde_json::json;

fn approvals_bytes(c: &mut IdentityRegistry, owner: &str, id: &str, who: &[&str]) -> u128 {
    // Mide lo que ocupan las aprobaciones de `who` (en el mismo contexto).
    ctx(owner, NEAR);
    c.flush_all();
    let antes = env::storage_usage();
    for w in who {
        assert!(c.nft_approve(id.into(), acc(w), None).is_none());
    }
    c.flush_all();
    (env::storage_usage() - antes) as u128 * BYTE
}

#[test]
fn transferir_limpia_wallet_pendiente_y_aprobaciones() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    let pagado = approvals_bytes(&mut c, ALICE, "0", &[OP, MARKET]);
    ctx(ALICE, 1);
    c.set_agent_wallet("0".into(), acc(WALLET), (NOW + 10).to_string());
    ctx(ALICE, 1);
    c.nft_transfer(acc(BOB), "0".into(), None, Some("venta".into()));
    assert_eq!(
        events(),
        vec![
            (
                "erc8004".into(),
                "metadata_set".into(),
                json!({"agent_id": "0", "metadata_key": "agentWallet", "metadata_value": ""})
            ),
            (
                "nep171".into(),
                "nft_transfer".into(),
                json!({"old_owner_id": ALICE, "new_owner_id": BOB, "token_ids": ["0"], "memo": "venta"})
            ),
        ]
    );
    // El storage de las aprobaciones vuelve entero al dueno anterior.
    assert_eq!(transfers(), vec![(ALICE.to_string(), pagado)]);
    let t = c.nft_token("0".into()).unwrap();
    assert_eq!(t.owner_id, acc(BOB));
    assert!(t.approved_account_ids.is_empty());
    assert_eq!(c.get_agent_wallet("0".into()), None);
    assert_eq!(c.get_pending_agent_wallet("0".into()), None);
    assert_eq!(c.nft_supply_for_owner(acc(ALICE)).0, 0);
    assert_eq!(c.nft_supply_for_owner(acc(BOB)).0, 1);
}

#[test]
fn transfiere_un_aprobado_con_su_approval_id_y_queda_authorized_id() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    approve(&mut c, ALICE, "0", OP);
    approve(&mut c, ALICE, "0", MARKET);
    assert_eq!(
        c.nft_token("0".into()).unwrap().approved_account_ids[&acc(MARKET)],
        1
    );
    ctx(MARKET, 1);
    c.nft_transfer(acc(BOB), "0".into(), Some(1), None);
    assert_eq!(events()[1].2["authorized_id"], MARKET);
    assert!(events()[1].2.get("memo").is_none());
}

#[test]
#[should_panic(expected = "ERC8004_ID_E19: approval_id no coincide")]
fn approval_id_viejo_es_e19() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    approve(&mut c, ALICE, "0", MARKET);
    approve(&mut c, ALICE, "0", MARKET);
    ctx(MARKET, 1);
    c.nft_transfer(acc(BOB), "0".into(), Some(0), None);
}

#[test]
fn el_dueno_puede_pasar_cualquier_approval_id() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    ctx(ALICE, 1);
    c.nft_transfer(acc(BOB), "0".into(), Some(99), None);
    assert_eq!(c.nft_token("0".into()).unwrap().owner_id, acc(BOB));
}

#[test]
#[should_panic(expected = "ERC8004_ID_E12: Not authorized")]
fn transferir_sin_ser_dueno_ni_aprobado_es_e12() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    ctx(BOB, 1);
    c.nft_transfer(acc(BOB), "0".into(), None, None);
}

#[test]
#[should_panic(expected = "ERC8004_ID_E12")]
fn la_aprobacion_no_sobrevive_a_la_transferencia() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    approve(&mut c, ALICE, "0", MARKET);
    transfer(&mut c, ALICE, BOB, "0");
    ctx(MARKET, 1);
    c.nft_transfer(acc(CAROL), "0".into(), None, None);
}

#[test]
#[should_panic(expected = "ERC8004_ID_E20")]
fn transferir_al_mismo_dueno_es_e20() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    transfer(&mut c, ALICE, ALICE, "0");
}

#[test]
#[should_panic(expected = "ERC8004_ID_E18")]
fn memo_largo_es_e18() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    ctx(ALICE, 1);
    c.nft_transfer(
        acc(BOB),
        "0".into(),
        None,
        Some("m".repeat(MAX_MEMO_LEN + 1)),
    );
}

#[test]
fn memo_al_tope_vale() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    ctx(ALICE, 1);
    c.nft_transfer(acc(BOB), "0".into(), None, Some("m".repeat(MAX_MEMO_LEN)));
}

#[test]
#[should_panic(expected = "Requires attached deposit of exactly 1 yoctoNEAR")]
fn transferir_sin_un_yocto() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    ctx(ALICE, 2);
    c.nft_transfer(acc(BOB), "0".into(), None, None);
}

#[test]
#[should_panic(expected = "ERC8004_ID_E11")]
fn transferir_inexistente_es_e11() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    transfer(&mut c, ALICE, BOB, "1");
}

#[test]
fn next_approval_id_no_vuelve_a_cero_al_transferir() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    approve(&mut c, ALICE, "0", MARKET);
    approve(&mut c, ALICE, "0", OP);
    transfer(&mut c, ALICE, BOB, "0");
    approve(&mut c, BOB, "0", MARKET);
    assert_eq!(
        c.nft_token("0".into()).unwrap().approved_account_ids[&acc(MARKET)],
        2
    );
}

#[test]
fn transfer_call_llama_al_receptor_y_al_resolve() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    let pagado = approvals_bytes(&mut c, ALICE, "0", &[OP]);
    ctx(ALICE, 1);
    let _ = c.nft_transfer_call(acc(MARKET), "0".into(), None, None, "listar".into());
    // El storage de las aprobaciones no vuelve todavia: lo decide el resolve.
    assert_eq!(transfers(), vec![]);
    let calls = function_calls();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].0, MARKET);
    assert_eq!(calls[0].1, "nft_on_transfer");
    assert_eq!(
        calls[0].2,
        json!({"sender_id": ALICE, "previous_owner_id": ALICE, "token_id": "0", "msg": "listar"})
    );
    assert_eq!(calls[0].3, GAS_FOR_NFT_ON_TRANSFER);
    assert_eq!(calls[1].0, CONTRACT);
    assert_eq!(calls[1].1, "nft_resolve_transfer");
    assert_eq!(calls[1].3, GAS_FOR_RESOLVE_TRANSFER);
    assert_eq!(calls[1].2["previous_owner_id"], ALICE);
    assert_eq!(calls[1].2["receiver_id"], MARKET);
    assert_eq!(calls[1].2["approved_account_ids"], json!({OP: 0}));
    assert_eq!(calls[1].2["approvals_refund"], pagado.to_string());
}

#[test]
#[should_panic(expected = "ERC8004_ID_E21")]
fn transfer_call_con_poco_gas_es_e21() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    ctx_full(
        ALICE,
        ALICE,
        1,
        NOW,
        MIN_GAS_NFT_TRANSFER_CALL.saturating_sub(Gas::from_gas(1)),
    );
    let _ = c.nft_transfer_call(acc(MARKET), "0".into(), None, None, "".into());
}

/// Transfiere con `nft_transfer_call` a MARKET (ALICE habia aprobado a OP) y devuelve lo que se mando al resolve.
fn transfer_call(c: &mut IdentityRegistry) -> (Option<BTreeMap<AccountId, u64>>, u128) {
    reg(c, ALICE);
    let pagado = approvals_bytes(c, ALICE, "0", &[OP]);
    ctx(ALICE, 1);
    let _ = c.nft_transfer_call(acc(MARKET), "0".into(), None, None, "".into());
    let mut m = BTreeMap::new();
    m.insert(acc(OP), 0u64);
    (Some(m), pagado)
}

#[test]
fn resolve_true_lo_devuelve_con_sus_aprobaciones() {
    let mut c = new_contract();
    let (a, pagado) = transfer_call(&mut c);
    // Mientras lo tuvo, MARKET aprobo a CAROL: eso se borra y su storage vuelve a MARKET.
    let de_market = approvals_bytes(&mut c, MARKET, "0", &[CAROL]);
    ctx_callback(PromiseResult::Successful(b"true".to_vec()));
    assert!(!c.nft_resolve_transfer(acc(ALICE), acc(MARKET), "0".into(), a, U128(pagado)));
    assert_eq!(transfers(), vec![(MARKET.to_string(), de_market)]);
    let t = c.nft_token("0".into()).unwrap();
    assert_eq!(t.owner_id, acc(ALICE));
    assert_eq!(
        t.approved_account_ids.keys().collect::<Vec<_>>(),
        vec![&acc(OP)]
    );
    assert_eq!(c.get_agent_wallet("0".into()), None);
    assert_eq!(
        event_names(),
        vec!["metadata_set".to_string(), "nft_transfer".to_string()]
    );
    assert_eq!(
        events()[1].2,
        json!({"old_owner_id": MARKET, "new_owner_id": ALICE, "token_ids": ["0"]})
    );
    assert_eq!(c.nft_supply_for_owner(acc(MARKET)).0, 0);
    assert_eq!(c.nft_supply_for_owner(acc(ALICE)).0, 1);
}

#[test]
fn resolve_false_deja_la_transferencia_y_devuelve_las_aprobaciones() {
    let mut c = new_contract();
    let (a, pagado) = transfer_call(&mut c);
    ctx_callback(PromiseResult::Successful(b"false".to_vec()));
    assert!(c.nft_resolve_transfer(acc(ALICE), acc(MARKET), "0".into(), a, U128(pagado)));
    assert_eq!(transfers(), vec![(ALICE.to_string(), pagado)]);
    assert_eq!(c.nft_token("0".into()).unwrap().owner_id, acc(MARKET));
    assert_eq!(event_names(), Vec::<String>::new());
}

#[test]
fn resolve_con_receptor_que_fallo_o_respuesta_rara_lo_devuelve() {
    for r in [
        PromiseResult::Failed,
        PromiseResult::Successful(b"\"no\"".to_vec()),
        PromiseResult::Successful(b"".to_vec()),
        PromiseResult::Successful(vec![b't'; 100]),
    ] {
        let mut c = new_contract();
        let (a, pagado) = transfer_call(&mut c);
        ctx_callback(r);
        assert!(!c.nft_resolve_transfer(acc(ALICE), acc(MARKET), "0".into(), a, U128(pagado)));
        assert_eq!(c.nft_token("0".into()).unwrap().owner_id, acc(ALICE));
    }
}

#[test]
fn resolve_si_el_receptor_ya_lo_paso_queda_donde_esta() {
    let mut c = new_contract();
    let (a, pagado) = transfer_call(&mut c);
    transfer(&mut c, MARKET, CAROL, "0");
    ctx_callback(PromiseResult::Successful(b"true".to_vec()));
    assert!(c.nft_resolve_transfer(acc(ALICE), acc(MARKET), "0".into(), a, U128(pagado)));
    assert_eq!(c.nft_token("0".into()).unwrap().owner_id, acc(CAROL));
    assert_eq!(transfers(), vec![(ALICE.to_string(), pagado)]);
}

#[test]
fn approve_cobra_lo_medido_y_con_msg_llama_nft_on_approve() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    ctx(ALICE, NEAR);
    c.flush_all();
    let antes = env::storage_usage();
    let p = c.nft_approve("0".into(), acc(MARKET), Some("listar".into()));
    c.flush_all();
    let cobrado = (env::storage_usage() - antes) as u128 * BYTE;
    assert!(p.is_some());
    p.unwrap().detach();
    assert_eq!(transfers(), vec![(ALICE.to_string(), NEAR - cobrado)]);
    let call = function_calls()
        .into_iter()
        .find(|c| c.1 == "nft_on_approve")
        .unwrap();
    assert_eq!(call.0, MARKET);
    assert_eq!(
        call.2,
        json!({"token_id": "0", "owner_id": ALICE, "approval_id": 0, "msg": "listar"})
    );
    assert_eq!(call.3, GAS_FOR_NFT_ON_APPROVE);
    assert!(c.nft_is_approved("0".into(), acc(MARKET), None));
    assert!(c.nft_is_approved("0".into(), acc(MARKET), Some(0)));
    assert!(!c.nft_is_approved("0".into(), acc(MARKET), Some(1)));
    assert!(!c.nft_is_approved("0".into(), acc(OP), None));
    assert!(!c.nft_is_approved("9".into(), acc(MARKET), None));
}

#[test]
#[should_panic(expected = "ERC8004_ID_E22: solo el dueno del agente")]
fn approve_de_un_aprobado_es_e22() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    approve(&mut c, ALICE, "0", OP);
    approve(&mut c, OP, "0", MARKET);
}

#[test]
#[should_panic(expected = "ERC8004_ID_E07")]
fn approve_sin_yocto_es_e07() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    ctx(ALICE, 0);
    c.nft_approve("0".into(), acc(OP), None);
}

#[test]
#[should_panic(expected = "ERC8004_ID_E06")]
fn approve_con_poco_deposito_es_e06() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    ctx(ALICE, 1);
    c.nft_approve("0".into(), acc(OP), None);
}

#[test]
fn hasta_ocho_aprobaciones_y_reaprobar_no_cuenta() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    for i in 0..MAX_APPROVALS {
        approve(&mut c, ALICE, "0", &format!("a{i}.testnet"));
    }
    approve(&mut c, ALICE, "0", "a0.testnet");
    assert_eq!(
        c.nft_token("0".into()).unwrap().approved_account_ids.len(),
        MAX_APPROVALS
    );
}

#[test]
#[should_panic(expected = "ERC8004_ID_E23")]
fn la_novena_aprobacion_es_e23() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    for i in 0..=MAX_APPROVALS {
        approve(&mut c, ALICE, "0", &format!("a{i}.testnet"));
    }
}

#[test]
fn revoke_y_revoke_all_devuelven_lo_liberado_al_dueno() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    let uno = approvals_bytes(&mut c, ALICE, "0", &[OP]);
    let dos = approvals_bytes(&mut c, ALICE, "0", &[MARKET]);
    ctx(ALICE, 1);
    c.nft_revoke("0".into(), acc(MARKET));
    assert_eq!(transfers(), vec![(ALICE.to_string(), dos)]);
    assert!(!c.nft_is_approved("0".into(), acc(MARKET), None));
    // Revocar a quien no esta no hace nada.
    ctx(ALICE, 1);
    c.nft_revoke("0".into(), acc(CAROL));
    assert_eq!(transfers(), vec![]);
    ctx(ALICE, 1);
    c.nft_revoke("0".into(), acc(OP));
    assert_eq!(transfers(), vec![(ALICE.to_string(), uno)]);
    approve(&mut c, ALICE, "0", OP);
    approve(&mut c, ALICE, "0", MARKET);
    ctx(ALICE, 1);
    c.nft_revoke_all("0".into());
    assert_eq!(refunded_to(ALICE), uno + dos);
    assert!(
        c.nft_token("0".into())
            .unwrap()
            .approved_account_ids
            .is_empty()
    );
}

#[test]
#[should_panic(expected = "ERC8004_ID_E22")]
fn revoke_de_otro_es_e22() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    approve(&mut c, ALICE, "0", OP);
    ctx(OP, 1);
    c.nft_revoke("0".into(), acc(OP));
}

#[test]
#[should_panic(expected = "ERC8004_ID_E22")]
fn revoke_all_de_otro_es_e22() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    ctx(BOB, 1);
    c.nft_revoke_all("0".into());
}

#[test]
#[should_panic(expected = "Requires attached deposit of exactly 1 yoctoNEAR")]
fn revoke_sin_un_yocto() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    ctx(ALICE, 0);
    c.nft_revoke("0".into(), acc(OP));
}

#[test]
#[should_panic(expected = "Requires attached deposit of exactly 1 yoctoNEAR")]
fn revoke_all_sin_un_yocto() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    ctx(ALICE, 0);
    c.nft_revoke_all("0".into());
}

#[test]
fn resolve_que_lo_devuelve_limpia_la_wallet_que_puso_el_receptor() {
    let mut c = new_contract();
    let (a, pagado) = transfer_call(&mut c);
    // Mientras lo tuvo, MARKET (dueno) se puso como agentWallet.
    ctx(MARKET, 1);
    assert!(c.set_agent_wallet("0".into(), acc(MARKET), NOW.to_string()));
    ctx_callback(PromiseResult::Successful(b"true".to_vec()));
    assert!(!c.nft_resolve_transfer(acc(ALICE), acc(MARKET), "0".into(), a, U128(pagado)));
    assert_eq!(c.get_agent_wallet("0".into()), None);
    assert_eq!(events()[0].2["metadata_value"], "");
}
