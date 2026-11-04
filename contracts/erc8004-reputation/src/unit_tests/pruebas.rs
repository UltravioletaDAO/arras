//! La extension `feedback-proofs`: `give_feedback_with_proof`, su callback con las dos respuestas, el candado y las
//! vistas de las pruebas.

use super::common::*;
use crate::*;
use near_sdk::PromiseResult;
use near_sdk::json_types::U64;
use near_sdk::serde_json::{self, json};

fn dep() -> u128 {
    ReputationRegistry::feedback_proof_deposit()
}

fn call_give_proof(c: &mut ReputationRegistry, p: &Pedido, verifier: &str, proof: &str) {
    c.give_feedback_with_proof(
        p.agent_id.clone(),
        p.value.clone(),
        p.value_decimals,
        p.tag1.clone(),
        p.tag2.clone(),
        p.endpoint.clone(),
        p.feedback_uri.clone(),
        p.feedback_hash.clone(),
        acc(verifier),
        proof.into(),
    )
    .detach();
}

struct Args {
    p: PendingFeedback,
    verifier: AccountId,
    proof: String,
    nonce: U64,
}

fn give_proof_at(
    c: &mut ReputationRegistry,
    client: &str,
    deposit: u128,
    height: u64,
    proof: &str,
) -> Args {
    ctx_at(client, deposit, height);
    call_give_proof(c, &Pedido::default(), VERIFIER, proof);
    let a = call_args("on_feedback_proof_checked");
    Args {
        p: serde_json::from_value(a["p"].clone()).unwrap(),
        verifier: serde_json::from_value(a["verifier"].clone()).unwrap(),
        proof: serde_json::from_value(a["proof"].clone()).unwrap(),
        nonce: serde_json::from_value(a["nonce"].clone()).unwrap(),
    }
}

fn give_proof(c: &mut ReputationRegistry, client: &str) -> Args {
    give_proof_at(c, client, dep(), HEIGHT, PID)
}

fn cb(
    c: &mut ReputationRegistry,
    a: Args,
    identity: PromiseResult,
    verified: PromiseResult,
) -> Option<String> {
    ctx_callback(vec![identity, verified], HEIGHT + 3);
    c.on_feedback_proof_checked(a.p, a.verifier, a.proof, a.nonce)
}

/// El trato verificado: el cliente le pago al dueno del agente.
fn trato() -> PromiseResult {
    proof_resp(OWNER, "receiver", "em", 100_000)
}

#[test]
fn llama_en_paralelo_a_identity_y_al_verificador() {
    let mut c = new_contract();
    let a = give_proof(&mut c, CLIENT);
    let calls = function_calls();
    assert_eq!(calls.len(), 3);
    let check = calls.iter().find(|x| x.1 == "check_agent").unwrap();
    assert_eq!((check.0.as_str(), check.3), (IDENTITY, GAS_CHECK_AGENT));
    assert_eq!(check.2, json!({"agent_id": "7", "account_id": CLIENT}));
    let ver = calls
        .iter()
        .find(|x| x.1 == "verify_feedback_proof")
        .unwrap();
    assert_eq!(
        (ver.0.as_str(), ver.3, ver.4),
        (VERIFIER, GAS_VERIFY_PROOF, 0)
    );
    assert_eq!(ver.2, json!({"client": CLIENT, "proof": PID}));
    let cbk = calls
        .iter()
        .find(|x| x.1 == "on_feedback_proof_checked")
        .unwrap();
    assert_eq!((cbk.0.as_str(), cbk.3), (CONTRACT, GAS_ON_PROOF_CHECKED));
    assert_eq!(a.nonce.0, HEIGHT);
    assert!(c.is_proof_pending(acc(VERIFIER), PID.into(), acc(CLIENT)));
}

#[test]
fn aceptada_guarda_la_prueba_y_la_cuenta_en_los_dos_resumenes() {
    let mut c = new_contract();
    let a = give_proof(&mut c, CLIENT);
    assert_eq!(cb(&mut c, a, agent_ok(), trato()), Some("1".into()));
    let ev = events();
    assert_eq!(ev[0].0, "new_feedback");
    assert_eq!(
        ev[0].1["proof"],
        json!({"verifier": VERIFIER, "proof": PID, "counterparty": OWNER, "role": "receiver", "context": "em",
               "weight": "100000"})
    );
    assert!(!c.is_proof_pending(acc(VERIFIER), PID.into(), acc(CLIENT)));
    assert_eq!(
        serde_json::to_value(c.read_feedback_proof("7".into(), acc(CLIENT), "1".into())).unwrap(),
        ev[0].1["proof"]
    );
    assert_eq!(
        serde_json::to_value(c.get_proof_feedback(acc(VERIFIER), PID.into(), acc(CLIENT))).unwrap(),
        json!({"agent_id": "7", "client_address": CLIENT, "feedback_index": "1"})
    );
    assert_eq!(
        c.get_summary("7".into(), vec![acc(CLIENT)], None, None)
            .count
            .0,
        1
    );
    let v = c.get_verified_summary("7".into(), acc(VERIFIER), None, None, None, None);
    assert_eq!(
        (v.count.0, v.summary_value.as_str(), v.weight_sum.as_str()),
        (1, "87", "100000")
    );
    assert_eq!(
        serde_json::to_value(c.list_verified_feedback("7".into(), acc(VERIFIER), None, None))
            .unwrap(),
        json!([{"agent_id": "7", "client_address": CLIENT, "feedback_index": "1"}])
    );
}

#[test]
fn la_contraparte_puede_ser_la_agent_wallet() {
    let mut c = new_contract();
    let a = give_proof(&mut c, CLIENT);
    assert!(
        cb(
            &mut c,
            a,
            agent(false, Some(WALLET)),
            proof_resp(WALLET, "payer", "em", 1)
        )
        .is_some()
    );
}

fn rechazo(identity: PromiseResult, verified: PromiseResult, code: &str) {
    let mut c = new_contract();
    let extra = 999;
    let a = give_proof_at(&mut c, CLIENT, dep() + extra, HEIGHT, PID);
    ctx_callback(vec![identity, verified], HEIGHT + 3);
    c.flush_all();
    let antes = env::storage_usage();
    assert_eq!(
        c.on_feedback_proof_checked(a.p, a.verifier, a.proof, a.nonce),
        None
    );
    c.flush_all();
    // Solo se borro el candado.
    assert_eq!(antes - env::storage_usage(), PENDING_RECORD_BYTES);
    assert_eq!(logs(), vec![format!("erc8004:rechazo:{code}")]);
    assert_eq!(transfers(), vec![(CLIENT.to_string(), dep() + extra)]);
    assert!(!c.is_proof_pending(acc(VERIFIER), PID.into(), acc(CLIENT)));
    assert_eq!(
        c.get_proof_feedback(acc(VERIFIER), PID.into(), acc(CLIENT)),
        None
    );
    assert_eq!(c.get_last_index("7".into(), acc(CLIENT)).0, 0);
}

#[test]
fn la_identidad_rechaza_igual_que_en_el_nucleo() {
    rechazo(
        PromiseResult::Failed,
        trato(),
        "E20_VISTA_DE_IDENTIDAD_FALLO",
    );
    rechazo(raw(b"{}"), trato(), "E21_RESPUESTA_DE_IDENTIDAD_INVALIDA");
    rechazo(ok(json!(null)), trato(), "E22_AGENTE_INEXISTENTE");
    rechazo(agent(true, Some(OWNER)), trato(), "E23_AUTO_CALIFICACION");
}

#[test]
fn el_verificador_que_falla_es_e45() {
    rechazo(agent_ok(), PromiseResult::Failed, "E45_VERIFICADOR_FALLO");
}

#[test]
fn respuestas_raras_del_verificador_son_e46() {
    for r in [
        raw(b""),
        raw(b"null"),
        ok(json!({"counterparty": OWNER, "role": "receiver", "context": "em"})),
        ok(
            json!({"counterparty": OWNER, "role": "receiver", "context": "em", "weight": "1", "x": 1}),
        ),
        ok(json!({"counterparty": OWNER, "role": "Receiver", "context": "em", "weight": "1"})),
        ok(json!({"counterparty": OWNER, "role": "receiver", "context": "em", "weight": "01"})),
        raw(&vec![
            b' ';
            erc8004_common::types::MAX_FEEDBACK_PROOF_LEN + 1
        ]),
    ] {
        rechazo(agent_ok(), r, "E46_RESPUESTA_DEL_VERIFICADOR_INVALIDA");
    }
}

#[test]
fn una_contraparte_que_no_es_el_agente_es_e47() {
    rechazo(
        agent_ok(),
        proof_resp(OTHER, "receiver", "em", 1),
        "E47_NO_ES_EL_AGENTE",
    );
    rechazo(
        agent(false, None),
        proof_resp(WALLET, "receiver", "em", 1),
        "E47_NO_ES_EL_AGENTE",
    );
}

#[test]
fn la_agent_wallet_no_califica_a_su_agente_e48() {
    let mut c = new_contract();
    let a = give_proof(&mut c, WALLET);
    assert_eq!(cb(&mut c, a, agent(false, Some(WALLET)), trato()), None);
    assert_eq!(
        logs(),
        vec!["erc8004:rechazo:E48_AGENT_WALLET_CALIFICA".to_string()]
    );
}

#[test]
fn un_candado_ajeno_es_e44_y_no_lo_suelta() {
    let mut c = new_contract();
    let viejo = give_proof_at(&mut c, CLIENT, dep(), HEIGHT, PID);
    // El candado vencio y otra llamada lo piso.
    let nuevo = give_proof_at(&mut c, CLIENT, dep(), HEIGHT + PENDING_TTL_BLOCKS, PID);
    ctx_callback(vec![agent_ok(), trato()], HEIGHT + PENDING_TTL_BLOCKS + 1);
    assert_eq!(
        c.on_feedback_proof_checked(viejo.p, viejo.verifier, viejo.proof, viejo.nonce),
        None
    );
    assert_eq!(logs(), vec!["erc8004:rechazo:E44_REEMPLAZADO".to_string()]);
    assert_eq!(transfers(), vec![(CLIENT.to_string(), dep())]);
    // El candado sigue siendo del nuevo, que acepta.
    ctx_callback(vec![agent_ok(), trato()], HEIGHT + PENDING_TTL_BLOCKS + 2);
    assert!(c.is_proof_pending(acc(VERIFIER), PID.into(), acc(CLIENT)));
    assert_eq!(
        c.on_feedback_proof_checked(nuevo.p, nuevo.verifier, nuevo.proof, nuevo.nonce),
        Some("1".into())
    );
}

#[test]
#[should_panic(expected = "ERC8004_REP_E43")]
fn dos_en_vuelo_es_e43() {
    let mut c = new_contract();
    give_proof_at(&mut c, CLIENT, dep(), HEIGHT, PID);
    give_proof_at(&mut c, CLIENT, dep(), HEIGHT + PENDING_TTL_BLOCKS - 1, PID);
}

#[test]
fn otro_cliente_u_otra_prueba_no_choca_con_el_candado() {
    let mut c = new_contract();
    give_proof_at(&mut c, CLIENT, dep(), HEIGHT, PID);
    give_proof_at(&mut c, CLIENT2, dep(), HEIGHT, PID);
    give_proof_at(&mut c, CLIENT, dep(), HEIGHT, "otro-pago");
}

#[test]
#[should_panic(expected = "ERC8004_REP_E42")]
fn una_prueba_usada_es_e42_sincronico() {
    let mut c = new_contract();
    let a = give_proof(&mut c, CLIENT);
    cb(&mut c, a, agent_ok(), trato()).unwrap();
    give_proof(&mut c, CLIENT);
}

#[test]
fn la_prueba_usada_se_vuelve_a_mirar_en_el_callback_e42() {
    let mut c = new_contract();
    let a = give_proof(&mut c, CLIENT);
    // La marca aparecio mientras volaba (en la wasm no puede pasar con el candado; el callback igual la mira).
    c.proof_used
        .insert(proof_key(&acc(VERIFIER), PID, &acc(CLIENT)), (7, 1));
    assert_eq!(cb(&mut c, a, agent_ok(), trato()), None);
    assert_eq!(
        logs(),
        vec!["erc8004:rechazo:E42_PRUEBA_YA_USADA".to_string()]
    );
}

#[test]
fn revocar_no_libera_la_prueba() {
    let mut c = new_contract();
    let a = give_proof(&mut c, CLIENT);
    cb(&mut c, a, agent_ok(), trato()).unwrap();
    ctx(CLIENT, 1);
    c.revoke_feedback("7".into(), "1".into());
    let v = c.get_verified_summary("7".into(), acc(VERIFIER), None, None, None, None);
    assert_eq!((v.count.0, v.weight_sum.as_str()), (0, "0"));
    assert!(
        c.get_proof_feedback(acc(VERIFIER), PID.into(), acc(CLIENT))
            .is_some()
    );
    assert_eq!(
        c.list_verified_feedback("7".into(), acc(VERIFIER), None, None)
            .len(),
        1
    );
}

#[test]
#[should_panic(expected = "ERC8004_REP_E40")]
fn proof_vacia_es_e40() {
    let mut c = new_contract();
    give_proof_at(&mut c, CLIENT, dep(), HEIGHT, "");
}

#[test]
#[should_panic(expected = "ERC8004_REP_E40")]
fn proof_con_espacio_es_e40() {
    let mut c = new_contract();
    give_proof_at(&mut c, CLIENT, dep(), HEIGHT, "a b");
}

#[test]
#[should_panic(expected = "ERC8004_REP_E40")]
fn proof_larga_es_e40() {
    let mut c = new_contract();
    give_proof_at(&mut c, CLIENT, dep(), HEIGHT, &"p".repeat(129));
}

#[test]
#[should_panic(expected = "ERC8004_REP_E41")]
fn el_registro_como_verificador_es_e41() {
    let mut c = new_contract();
    ctx(CLIENT, dep());
    call_give_proof(&mut c, &Pedido::default(), CONTRACT, PID);
}

#[test]
#[should_panic(expected = "ERC8004_REP_E09")]
fn con_el_deposito_del_nucleo_no_alcanza_e09() {
    let mut c = new_contract();
    give_proof_at(&mut c, CLIENT, dep() - 1, HEIGHT, PID);
}

#[test]
#[should_panic(expected = "ERC8004_REP_E10")]
fn con_el_gas_del_nucleo_no_alcanza_e10() {
    let mut c = new_contract();
    ctx_full(
        CLIENT,
        CLIENT,
        dep(),
        HEIGHT,
        MIN_GAS_GIVE_FEEDBACK_WITH_PROOF.saturating_sub(Gas::from_gas(1)),
    );
    call_give_proof(&mut c, &Pedido::default(), VERIFIER, PID);
}

#[test]
#[should_panic(expected = "ERC8004_REP_E02")]
fn las_guardas_del_nucleo_tambien_valen() {
    let mut c = new_contract();
    ctx(CLIENT, dep());
    call_give_proof(
        &mut c,
        &Pedido {
            value: "01".into(),
            ..Pedido::default()
        },
        VERIFIER,
        PID,
    );
}

#[test]
fn get_verified_summary_filtra_por_clientes_tags_y_context() {
    let mut c = new_contract();
    for (client, proof, ctxs, w) in [
        (CLIENT, "p1", "em", 10u128),
        (CLIENT2, "p2", "em", 20),
        (CLIENT, "p3", "otro", 40),
    ] {
        let a = give_proof_at(&mut c, client, dep(), HEIGHT, proof);
        cb(
            &mut c,
            a,
            agent_ok(),
            proof_resp(OWNER, "receiver", ctxs, w),
        )
        .unwrap();
    }
    // Una sin prueba del mismo cliente no entra al resumen verificado.
    rate(&mut c, CLIENT, &pedido("7", "1", 0, "starred", ""));
    let s = |cl: Option<Vec<&str>>, ctxs: Option<&str>| {
        let v = c.get_verified_summary(
            "7".into(),
            acc(VERIFIER),
            cl.map(|v| v.into_iter().map(acc).collect()),
            None,
            None,
            ctxs.map(Into::into),
        );
        (v.count.0, v.weight_sum)
    };
    assert_eq!(s(None, None), (3, "70".into()));
    assert_eq!(s(Some(vec![]), None), (3, "70".into()));
    assert_eq!(s(Some(vec![CLIENT]), None), (2, "50".into()));
    assert_eq!(s(Some(vec![CLIENT, CLIENT]), None), (2, "50".into()));
    assert_eq!(s(None, Some("em")), (2, "30".into()));
    assert_eq!(s(None, Some("")), (0, "0".into()));
    let otro = c.get_verified_summary("7".into(), acc(OTHER), None, None, None, None);
    assert_eq!(otro.count.0, 0);
    let t = c.get_verified_summary(
        "7".into(),
        acc(VERIFIER),
        None,
        Some("nada".into()),
        None,
        None,
    );
    assert_eq!(t.count.0, 0);
    assert_eq!(
        c.get_summary("7".into(), vec![acc(CLIENT), acc(CLIENT2)], None, None)
            .count
            .0,
        4
    );
    assert_eq!(
        c.list_verified_feedback("7".into(), acc(VERIFIER), Some(1), Some(1))
            .len(),
        1
    );
}

#[test]
fn ust3_peor_caso_de_una_calificacion_con_prueba() {
    let client = "c".repeat(64);
    let verifier = "v".repeat(64);
    let counterparty = "o".repeat(64);
    let proof = "p".repeat(128);
    let mut c = new_contract();
    ctx(&client, dep());
    let p = Pedido {
        agent_id: u64::MAX.to_string(),
        tag1: Some("t".repeat(64)),
        tag2: Some("u".repeat(64)),
        ..Pedido::default()
    };
    call_give_proof(&mut c, &p, &verifier, &proof);
    let a = call_args("on_feedback_proof_checked");
    let pend: PendingFeedback = serde_json::from_value(a["p"].clone()).unwrap();
    let nonce: U64 = serde_json::from_value(a["nonce"].clone()).unwrap();
    ctx_callback(
        vec![
            ok(json!({"owner_id": counterparty, "agent_wallet": null, "authorized": false})),
            proof_resp(&counterparty, &"r".repeat(32), &"x".repeat(32), u128::MAX),
        ],
        HEIGHT + 3,
    );
    c.flush_all();
    let antes = env::storage_usage();
    assert!(
        c.on_feedback_proof_checked(pend, acc(&verifier), proof, nonce)
            .is_some(),
        "{:?}",
        logs()
    );
    c.flush_all();
    assert_eq!(
        env::storage_usage() + PENDING_RECORD_BYTES - antes,
        FEEDBACK_PROOF_MAX_BYTES
    );
    assert_eq!(FEEDBACK_PROOF_MAX_BYTES, 1_472);
    assert_eq!(transfers(), vec![]);
}

#[test]
fn is_proof_pending_vence_con_el_ttl() {
    let mut c = new_contract();
    give_proof_at(&mut c, CLIENT, dep(), HEIGHT, PID);
    ctx_at(OTHER, 0, HEIGHT + PENDING_TTL_BLOCKS - 1);
    assert!(c.is_proof_pending(acc(VERIFIER), PID.into(), acc(CLIENT)));
    ctx_at(OTHER, 0, HEIGHT + PENDING_TTL_BLOCKS);
    assert!(!c.is_proof_pending(acc(VERIFIER), PID.into(), acc(CLIENT)));
    assert!(!c.is_proof_pending(acc(VERIFIER), PID.into(), acc(CLIENT2)));
}
