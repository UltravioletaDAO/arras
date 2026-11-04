//! El Validation Registry (BORRADOR): cada guarda de `validation_request`, su callback y `validation_response`.

use crate::*;
use near_sdk::mock::MockAction;
use near_sdk::serde_json::{self, Value, json};
use near_sdk::test_utils::{VMContextBuilder, get_created_receipts, get_logs};
use near_sdk::{NearToken, PromiseResult, testing_env};

const CONTRACT: &str = "validation-registry.erc-8004.testnet";
const IDENTITY: &str = "identity-registry.erc-8004.testnet";
const OWNER: &str = "dueno.testnet";
const VALIDATOR: &str = "validador.testnet";
const OTHER: &str = "tercero.testnet";
const H1: &str = "00ff10a0b1c2d3e4f5061728394a5b6c7d8e9fa0b1c2d3e4f5061728394a5b6c";
const H2: &str = "1111111111111111111111111111111111111111111111111111111111111111";
const RURI: &str = "ipfs://bafypedido";
const HEIGHT: u64 = 1_000;
const NOW: u64 = 1_793_491_200_000_000_000;
const BYTE: u128 = 10_000_000_000_000_000_000;
const NEAR: u128 = 1_000_000_000_000_000_000_000_000;

fn acc(s: &str) -> AccountId {
    s.parse().unwrap()
}

fn ctx_g(pred: &str, dep: u128, height: u64, gas: Gas) {
    let usage = env::storage_usage();
    testing_env!(
        VMContextBuilder::new()
            .storage_usage(usage)
            .current_account_id(acc(CONTRACT))
            .predecessor_account_id(acc(pred))
            .attached_deposit(NearToken::from_yoctonear(dep))
            .block_height(height)
            .block_timestamp(NOW)
            .prepaid_gas(gas)
            .build()
    );
}

fn ctx(pred: &str, dep: u128) {
    ctx_g(pred, dep, HEIGHT, Gas::from_tgas(300));
}

fn ctx_cb(r: PromiseResult) {
    let usage = env::storage_usage();
    testing_env!(
        VMContextBuilder::new()
            .storage_usage(usage)
            .current_account_id(acc(CONTRACT))
            .predecessor_account_id(acc(CONTRACT))
            .block_height(HEIGHT + 2)
            .block_timestamp(NOW + 7)
            .build(),
        near_sdk::test_vm_config(),
        near_sdk::RuntimeFeesConfig::test(),
        Default::default(),
        vec![r]
    );
}

fn new_contract() -> ValidationRegistry {
    ctx(CONTRACT, 0);
    ValidationRegistry::new(acc(IDENTITY))
}

fn dep() -> u128 {
    REQUEST_MAX_BYTES as u128 * BYTE
}

fn calls() -> Vec<(String, String, Value, Gas)> {
    let mut out = vec![];
    for r in get_created_receipts() {
        for a in r.actions {
            if let MockAction::FunctionCallWeight {
                method_name,
                args,
                prepaid_gas,
                ..
            } = a
            {
                out.push((
                    r.receiver_id.to_string(),
                    String::from_utf8(method_name).unwrap(),
                    serde_json::from_slice(&args).unwrap(),
                    prepaid_gas,
                ));
            }
        }
    }
    out
}

fn transfers() -> Vec<(String, u128)> {
    let mut out = vec![];
    for r in get_created_receipts() {
        for a in r.actions {
            if let MockAction::Transfer { deposit, .. } = a {
                out.push((r.receiver_id.to_string(), deposit.as_yoctonear()));
            }
        }
    }
    out
}

fn request(c: &mut ValidationRegistry, who: &str, d: u128, hash: &str) -> (PendingRequest, U64) {
    ctx(who, d);
    c.validation_request(acc(VALIDATOR), "7".into(), RURI.into(), hash.into())
        .detach();
    let a = calls()
        .into_iter()
        .find(|x| x.1 == "on_request_checked")
        .unwrap()
        .2;
    (
        serde_json::from_value(a["p"].clone()).unwrap(),
        serde_json::from_value(a["nonce"].clone()).unwrap(),
    )
}

fn agent(authorized: bool) -> PromiseResult {
    PromiseResult::Successful(
        serde_json::to_vec(
            &json!({"owner_id": OWNER, "agent_wallet": OWNER, "authorized": authorized}),
        )
        .unwrap(),
    )
}

fn accepted(c: &mut ValidationRegistry, hash: &str) {
    let (p, n) = request(c, OWNER, dep(), hash);
    ctx_cb(agent(true));
    assert!(c.on_request_checked(p, n));
}

#[test]
fn pedido_aceptado_llama_check_agent_guarda_y_emite() {
    let mut c = new_contract();
    let (p, n) = request(&mut c, OWNER, dep() + 9, H1);
    let cl = calls();
    assert_eq!(
        (cl[0].0.as_str(), cl[0].1.as_str(), cl[0].3),
        (IDENTITY, "check_agent", GAS_CHECK_AGENT)
    );
    assert_eq!(cl[0].2, json!({"agent_id": "7", "account_id": OWNER}));
    assert_eq!(cl[1].3, GAS_ON_REQUEST_CHECKED);
    assert!(c.is_request_pending(H1.into()));
    ctx_cb(agent(true));
    assert!(c.on_request_checked(p, n));
    // Lo cobrado: el Validation (validador de 17), los dos Vector nuevos y sus elementos.
    let r = erc8004_common::storage::record_bytes;
    let bytes = r(33, 4 + 17 + 8 + 1 + 1 + 4 + 8 + 1)
        + r(9, 17)
        + r(13, 32)
        + r(1 + 4 + 17, 41)
        + r(37, 32);
    assert_eq!(bytes, 480);
    assert_eq!(
        transfers(),
        vec![(OWNER.to_string(), dep() + 9 - bytes as u128 * BYTE)]
    );
    assert!(!c.is_request_pending(H1.into()));
    let ev = get_logs();
    assert_eq!(
        ev,
        vec![format!(
            r#"EVENT_JSON:{{"standard":"erc8004","version":"1.0.0","event":"validation_request","data":[{{"validator_address":"{VALIDATOR}","agent_id":"7","request_uri":"{RURI}","request_hash":"{H1}"}}]}}"#
        )]
    );
    assert_eq!(
        serde_json::to_value(c.get_validation_status(H1.into())).unwrap(),
        json!({"validator_address": VALIDATOR, "agent_id": "7", "response": 0, "response_hash": null, "tag": "",
               "last_update": (NOW + 7).to_string(), "has_response": false})
    );
    assert_eq!(
        c.get_agent_validations("7".into(), None, None),
        vec![H1.to_string()]
    );
    assert_eq!(
        c.get_validator_requests(acc(VALIDATOR), None, None),
        vec![H1.to_string()]
    );
}

fn rechazo(r: PromiseResult, code: &str) {
    let mut c = new_contract();
    let (p, n) = request(&mut c, OWNER, dep() + 3, H1);
    ctx_cb(r);
    assert!(!c.on_request_checked(p, n));
    assert_eq!(get_logs(), vec![format!("erc8004:rechazo:{code}")]);
    assert_eq!(transfers(), vec![(OWNER.to_string(), dep() + 3)]);
    assert_eq!(c.get_validation_status(H1.into()), None);
    assert!(!c.is_request_pending(H1.into()));
}

#[test]
fn rechazos_del_callback() {
    rechazo(PromiseResult::Failed, "E20_VISTA_DE_IDENTIDAD_FALLO");
    rechazo(
        PromiseResult::Successful(b"{}".to_vec()),
        "E21_RESPUESTA_DE_IDENTIDAD_INVALIDA",
    );
    rechazo(
        PromiseResult::Successful(vec![b' '; MAX_AGENT_CHECK_LEN + 1]),
        "E21_RESPUESTA_DE_IDENTIDAD_INVALIDA",
    );
    rechazo(
        PromiseResult::Successful(b"null".to_vec()),
        "E22_AGENTE_INEXISTENTE",
    );
    rechazo(agent(false), "E23_NO_AUTORIZADO");
}

#[test]
fn candado_ajeno_es_e30() {
    let mut c = new_contract();
    let (p, n) = request(&mut c, OWNER, dep(), H1);
    ctx_g(
        OWNER,
        dep(),
        HEIGHT + PENDING_TTL_BLOCKS,
        Gas::from_tgas(300),
    );
    c.validation_request(acc(VALIDATOR), "7".into(), RURI.into(), H1.into())
        .detach();
    ctx_cb(agent(true));
    assert!(!c.on_request_checked(p, n));
    assert_eq!(
        get_logs(),
        vec!["erc8004:rechazo:E30_REEMPLAZADO".to_string()]
    );
    assert!(
        c.is_request_pending(H1.into()) || env::block_height() >= HEIGHT + PENDING_TTL_BLOCKS * 2
    );
}

#[test]
fn el_callback_vuelve_a_mirar_si_existe() {
    let mut c = new_contract();
    let (p, n) = request(&mut c, OWNER, dep(), H1);
    c.validations.insert(
        parse_hash(H1).unwrap(),
        Validation {
            validator: acc(OTHER),
            agent_id: 1,
            response: 0,
            response_hash: None,
            tag: String::new(),
            last_update: 0,
            has_response: false,
        },
    );
    ctx_cb(agent(true));
    assert!(!c.on_request_checked(p, n));
    assert_eq!(get_logs(), vec!["erc8004:rechazo:E04_EXISTE".to_string()]);
}

macro_rules! panics {
    ($name:ident, $msg:literal, $body:expr) => {
        #[test]
        #[should_panic(expected = $msg)]
        fn $name() {
            #[allow(clippy::redundant_closure_call)]
            ($body)();
        }
    };
}

panics!(agent_id_invalido_e01, "ERC8004_VAL_E01", || {
    let mut c = new_contract();
    ctx(OWNER, dep());
    c.validation_request(acc(VALIDATOR), "07".into(), RURI.into(), H1.into())
        .detach();
});
panics!(request_uri_vacia_e02, "ERC8004_VAL_E02", || {
    let mut c = new_contract();
    ctx(OWNER, dep());
    c.validation_request(acc(VALIDATOR), "7".into(), "".into(), H1.into())
        .detach();
});
panics!(request_uri_larga_e02, "ERC8004_VAL_E02", || {
    let mut c = new_contract();
    ctx(OWNER, dep());
    c.validation_request(
        acc(VALIDATOR),
        "7".into(),
        format!("a:{}", "b".repeat(MAX_REQUEST_URI_LEN - 1)),
        H1.into(),
    )
    .detach();
});
panics!(hash_invalido_e03, "ERC8004_VAL_E03", || {
    let mut c = new_contract();
    ctx(OWNER, dep());
    c.validation_request(acc(VALIDATOR), "7".into(), RURI.into(), H1.to_uppercase())
        .detach();
});
panics!(hash_repetido_e04, "ERC8004_VAL_E04: exists", || {
    let mut c = new_contract();
    accepted(&mut c, H1);
    request(&mut c, OWNER, dep(), H1);
});
panics!(en_vuelo_e05, "ERC8004_VAL_E05", || {
    let mut c = new_contract();
    request(&mut c, OWNER, dep(), H1);
    ctx_g(
        OWNER,
        dep(),
        HEIGHT + PENDING_TTL_BLOCKS - 1,
        Gas::from_tgas(300),
    );
    c.validation_request(acc(VALIDATOR), "7".into(), RURI.into(), H1.into())
        .detach();
});
panics!(el_registro_como_validador_e06, "ERC8004_VAL_E06", || {
    let mut c = new_contract();
    ctx(OWNER, dep());
    c.validation_request(acc(CONTRACT), "7".into(), RURI.into(), H1.into())
        .detach();
});
panics!(deposito_corto_e07, "ERC8004_VAL_E07", || {
    let mut c = new_contract();
    request(&mut c, OWNER, dep() - 1, H1);
});
panics!(gas_corto_e08, "ERC8004_VAL_E08", || {
    let mut c = new_contract();
    ctx_g(
        OWNER,
        dep(),
        HEIGHT,
        MIN_GAS_VALIDATION_REQUEST.saturating_sub(Gas::from_gas(1)),
    );
    c.validation_request(acc(VALIDATOR), "7".into(), RURI.into(), H1.into())
        .detach();
});
panics!(init_con_el_propio_e15, "ERC8004_VAL_E15", || {
    ctx(CONTRACT, 0);
    ValidationRegistry::new(acc(CONTRACT));
});

#[test]
fn respuesta_del_validador_varias_veces() {
    let mut c = new_contract();
    accepted(&mut c, H1);
    accepted(&mut c, H2);
    ctx(VALIDATOR, NEAR);
    c.validation_response(
        H1.into(),
        100,
        Some("ipfs://bafyprueba".into()),
        Some(H2.into()),
        Some("soft".into()),
    );
    let st = c.get_validation_status(H1.into()).unwrap();
    assert_eq!(
        (st.response, st.has_response, st.tag.as_str()),
        (100, true, "soft")
    );
    assert_eq!(st.response_hash, Some(H2.to_string()));
    assert_eq!(
        get_logs(),
        vec![format!(
            r#"EVENT_JSON:{{"standard":"erc8004","version":"1.0.0","event":"validation_response","data":[{{"validator_address":"{VALIDATOR}","agent_id":"7","request_hash":"{H1}","response":100,"response_uri":"ipfs://bafyprueba","response_hash":"{H2}","tag":"soft"}}]}}"#
        )]
    );
    ctx(VALIDATOR, 1);
    c.validation_response(H1.into(), 40, None, None, None);
    // Lo liberado (el tag y el hash) vuelve al validador con su yocto.
    assert_eq!(
        transfers(),
        vec![(VALIDATOR.to_string(), 1 + (4 + 32) as u128 * BYTE)]
    );
    ctx(VALIDATOR, NEAR);
    c.validation_response(H2.into(), 91, None, None, Some("hard".into()));
    // floor((40 + 91) / 2) = 65
    let s = c.get_summary("7".into(), None, None);
    assert_eq!((s.count.0, s.average_response), (2, 65));
    assert_eq!(
        c.get_summary("7".into(), Some(vec![acc(OTHER)]), None)
            .count
            .0,
        0
    );
    assert_eq!(
        c.get_summary("7".into(), Some(vec![]), Some("".into()))
            .count
            .0,
        2
    );
    assert_eq!(
        c.get_summary("7".into(), Some(vec![acc(VALIDATOR)]), Some("hard".into()))
            .average_response,
        91
    );
    assert_eq!(c.get_summary("8".into(), None, None).count.0, 0);
}

#[test]
fn sin_respuesta_no_cuenta_en_el_resumen() {
    let mut c = new_contract();
    accepted(&mut c, H1);
    assert_eq!(c.get_summary("7".into(), None, None).count.0, 0);
}

panics!(respuesta_sin_yocto_e09, "ERC8004_VAL_E09", || {
    let mut c = new_contract();
    accepted(&mut c, H1);
    ctx(VALIDATOR, 0);
    c.validation_response(H1.into(), 1, None, None, None);
});
panics!(
    respuesta_desconocida_e10,
    "ERC8004_VAL_E10: unknown",
    || {
        let mut c = new_contract();
        ctx(VALIDATOR, 1);
        c.validation_response(H1.into(), 1, None, None, None);
    }
);
panics!(
    respuesta_de_otro_e11,
    "ERC8004_VAL_E11: not validator",
    || {
        let mut c = new_contract();
        accepted(&mut c, H1);
        ctx(OWNER, 1);
        c.validation_response(H1.into(), 1, None, None, None);
    }
);
panics!(respuesta_101_e12, "ERC8004_VAL_E12: resp>100", || {
    let mut c = new_contract();
    accepted(&mut c, H1);
    ctx(VALIDATOR, 1);
    c.validation_response(H1.into(), 101, None, None, None);
});
panics!(response_uri_invalida_e13, "ERC8004_VAL_E13", || {
    let mut c = new_contract();
    accepted(&mut c, H1);
    ctx(VALIDATOR, 1);
    c.validation_response(H1.into(), 1, Some("a b".into()), None, None);
});
panics!(response_hash_invalido_e03, "ERC8004_VAL_E03", || {
    let mut c = new_contract();
    accepted(&mut c, H1);
    ctx(VALIDATOR, 1);
    c.validation_response(H1.into(), 1, None, Some("0x00".into()), None);
});
panics!(tag_invalido_e14, "ERC8004_VAL_E14", || {
    let mut c = new_contract();
    accepted(&mut c, H1);
    ctx(VALIDATOR, 1);
    c.validation_response(H1.into(), 1, None, None, Some("t".repeat(65)));
});
panics!(tag_que_crece_sin_deposito_e07, "ERC8004_VAL_E07", || {
    let mut c = new_contract();
    accepted(&mut c, H1);
    ctx(VALIDATOR, 1);
    c.validation_response(H1.into(), 1, None, None, Some("t".repeat(64)));
});

#[test]
fn peor_caso_de_un_pedido_es_request_max_bytes() {
    let validator = "v".repeat(64);
    let mut c = new_contract();
    ctx(OWNER, dep());
    c.validation_request(
        acc(&validator),
        u64::MAX.to_string(),
        RURI.into(),
        H1.into(),
    )
    .detach();
    let a = calls()
        .into_iter()
        .find(|x| x.1 == "on_request_checked")
        .unwrap()
        .2;
    let p: PendingRequest = serde_json::from_value(a["p"].clone()).unwrap();
    let n: U64 = serde_json::from_value(a["nonce"].clone()).unwrap();
    ctx_cb(agent(true));
    let antes = c.usage();
    assert!(c.on_request_checked(p, n));
    assert_eq!(
        c.usage() + erc8004_common::storage::record_bytes(33, 8) - antes,
        REQUEST_MAX_BYTES
    );
    assert_eq!(transfers(), vec![]);
}

#[test]
fn config_dice_borrador() {
    let c = new_contract();
    let v = c.get_config();
    assert_eq!(v["status"], "BORRADOR");
    assert_eq!(v["identity_registry"], IDENTITY);
    assert_eq!(c.get_version(), "2.0.0");
    assert_eq!(c.get_identity_registry(), acc(IDENTITY));
    assert!(c.get_agent_validations("x".into(), None, None).is_empty());
    assert_eq!(c.get_validation_status("x".into()), None);
}

#[test]
fn el_cobro_no_pasa_del_deposito() {
    let mut c = new_contract();
    let (mut p, n) = request(&mut c, OWNER, dep(), H1);
    p.deposit = U128(3);
    ctx_cb(agent(true));
    assert!(c.on_request_checked(p, n));
    assert_eq!(transfers(), vec![]);
}
