//! `give_feedback`, su callback y `revoke_feedback`: cada guarda y cada resultado del callback.

use super::common::*;
use crate::*;
use near_sdk::PromiseResult;
use near_sdk::serde_json::{self, json};

#[test]
fn give_feedback_llama_check_agent_y_el_callback_con_su_gas() {
    let mut c = new_contract();
    let dep = ReputationRegistry::feedback_deposit() + 5;
    let p = give_with(&mut c, CLIENT, dep, &Pedido::default());
    let calls = function_calls();
    assert_eq!(calls.len(), 2);
    assert_eq!(
        (
            calls[0].0.as_str(),
            calls[0].1.as_str(),
            calls[0].3,
            calls[0].4
        ),
        (IDENTITY, "check_agent", GAS_CHECK_AGENT, 0)
    );
    assert_eq!(calls[0].2, json!({"agent_id": "7", "account_id": CLIENT}));
    assert_eq!(
        (calls[1].0.as_str(), calls[1].1.as_str(), calls[1].3),
        (CONTRACT, "on_feedback_checked", GAS_ON_FEEDBACK_CHECKED)
    );
    assert_eq!(p.client, acc(CLIENT));
    assert_eq!(p.deposit.0, dep);
    assert_eq!((p.tag1.as_str(), p.tag2.as_str()), ("starred", ""));
}

#[test]
fn el_cliente_es_el_predecessor_no_el_signer() {
    let mut c = new_contract();
    ctx_full(
        OTHER,
        CLIENT,
        ReputationRegistry::feedback_deposit(),
        HEIGHT,
        Gas::from_tgas(300),
    );
    call_give(&mut c, &Pedido::default());
    assert_eq!(call_args("check_agent")["account_id"], OTHER);
    assert_eq!(call_args("on_feedback_checked")["p"]["client"], OTHER);
}

#[test]
fn aceptada_da_el_indice_y_emite_new_feedback() {
    let mut c = new_contract();
    let p = give(&mut c, CLIENT, &Pedido::default());
    assert_eq!(callback(&mut c, p, agent_ok()), Some("1".into()));
    assert_eq!(
        events(),
        vec![(
            "new_feedback".into(),
            json!({
                "agent_id": "7", "client_address": CLIENT, "feedback_index": "1", "value": "87",
                "value_decimals": 0, "tag1": "starred", "tag2": "", "endpoint": "https://agente.example/mcp",
                "feedback_uri": "ipfs://bafyfeedback", "feedback_hash": HASH, "proof": null
            })
        )]
    );
    // Indices por par (agente, cliente), desde 1.
    assert_eq!(rate(&mut c, CLIENT, &Pedido::default()), "2");
    assert_eq!(rate(&mut c, CLIENT2, &Pedido::default()), "1");
    let otro = Pedido {
        agent_id: "8".into(),
        ..Pedido::default()
    };
    assert_eq!(rate(&mut c, CLIENT, &otro), "1");
    assert_eq!(c.get_last_index("7".into(), acc(CLIENT)).0, 2);
    assert_eq!(
        c.get_clients("7".into(), None, None),
        vec![acc(CLIENT), acc(CLIENT2)]
    );
}

#[test]
fn opcionales_ausentes_son_vacios_y_hash_null() {
    let mut c = new_contract();
    let p = Pedido {
        tag1: None,
        tag2: None,
        endpoint: None,
        feedback_uri: None,
        feedback_hash: None,
        ..Pedido::default()
    };
    rate(&mut c, CLIENT, &p);
    let e = &events()[0].1;
    assert_eq!(
        (e["tag1"].clone(), e["tag2"].clone()),
        (json!(""), json!(""))
    );
    assert_eq!(
        (e["endpoint"].clone(), e["feedback_uri"].clone()),
        (json!(""), json!(""))
    );
    assert_eq!(e["feedback_hash"], json!(null));
}

#[test]
fn aceptada_cobra_lo_medido_y_devuelve_el_resto() {
    let mut c = new_contract();
    let dep = ReputationRegistry::feedback_deposit() + 1234;
    let p = give_with(&mut c, CLIENT, dep, &Pedido::default());
    ctx_callback(vec![agent_ok()], HEIGHT + 2);
    c.flush_all();
    let antes = env::storage_usage();
    c.on_feedback_checked(p).unwrap();
    c.flush_all();
    let bytes = env::storage_usage() - antes;
    assert_eq!(
        transfers(),
        vec![(CLIENT.to_string(), dep - bytes as u128 * BYTE)]
    );
}

fn rechazo(r: PromiseResult, code: &str) {
    let mut c = new_contract();
    let dep = ReputationRegistry::feedback_deposit() + 777;
    let p = give_with(&mut c, CLIENT, dep, &Pedido::default());
    ctx_callback(vec![r], HEIGHT + 2);
    c.flush_all();
    let antes = env::storage_usage();
    assert_eq!(c.on_feedback_checked(p), None);
    c.flush_all();
    assert_eq!(env::storage_usage(), antes, "un rechazo no escribe nada");
    assert_eq!(logs(), vec![format!("erc8004:rechazo:{code}")]);
    // Devuelve todo lo adjuntado, no el deposito minimo.
    assert_eq!(transfers(), vec![(CLIENT.to_string(), dep)]);
    assert_eq!(c.get_last_index("7".into(), acc(CLIENT)).0, 0);
}

#[test]
fn la_vista_que_falla_es_e20() {
    rechazo(PromiseResult::Failed, "E20_VISTA_DE_IDENTIDAD_FALLO");
}

#[test]
fn respuestas_raras_son_e21() {
    for r in [
        raw(b""),
        raw(b"{"),
        raw(b"true"),
        raw(b"[]"),
        ok(json!({"owner_id": OWNER, "agent_wallet": null})),
        ok(json!({"owner_id": OWNER, "agent_wallet": null, "authorized": false, "extra": 1})),
        ok(json!({"owner_id": OWNER, "authorized": false})),
        ok(json!({"owner_id": OWNER, "agent_wallet": null, "authorized": "false"})),
        raw(&vec![b' '; erc8004_common::types::MAX_AGENT_CHECK_LEN + 1]),
    ] {
        rechazo(r, "E21_RESPUESTA_DE_IDENTIDAD_INVALIDA");
    }
}

#[test]
fn una_respuesta_al_tope_de_bytes_se_lee() {
    let mut c = new_contract();
    let p = give(&mut c, CLIENT, &Pedido::default());
    let mut body =
        serde_json::to_vec(&json!({"owner_id": OWNER, "agent_wallet": null, "authorized": false}))
            .unwrap();
    body.resize(erc8004_common::types::MAX_AGENT_CHECK_LEN, b' ');
    assert_eq!(callback(&mut c, p, raw(&body)), Some("1".into()));
}

#[test]
fn agente_inexistente_es_e22() {
    rechazo(ok(json!(null)), "E22_AGENTE_INEXISTENTE");
}

#[test]
fn el_dueno_o_un_aprobado_es_e23() {
    rechazo(agent(true, Some(OWNER)), "E23_AUTO_CALIFICACION");
    rechazo(agent(true, None), "E23_AUTO_CALIFICACION");
}

#[test]
fn un_agente_sin_wallet_acepta() {
    let mut c = new_contract();
    let p = give(&mut c, CLIENT, &Pedido::default());
    assert_eq!(callback(&mut c, p, agent(false, None)), Some("1".into()));
}

fn give_panics(p: Pedido) {
    let mut c = new_contract();
    ctx(CLIENT, ReputationRegistry::feedback_deposit());
    call_give(&mut c, &p);
}

#[test]
#[should_panic(expected = "ERC8004_REP_E01")]
fn agent_id_no_canonico_es_e01() {
    give_panics(Pedido {
        agent_id: "07".into(),
        ..Pedido::default()
    });
}

#[test]
#[should_panic(expected = "ERC8004_REP_E02")]
fn value_con_mas_es_e02() {
    give_panics(Pedido {
        value: "+1".into(),
        ..Pedido::default()
    });
}

#[test]
#[should_panic(expected = "ERC8004_REP_E02")]
fn value_menos_cero_es_e02() {
    give_panics(Pedido {
        value: "-0".into(),
        ..Pedido::default()
    });
}

#[test]
#[should_panic(expected = "ERC8004_REP_E02")]
fn value_decimal_es_e02() {
    give_panics(Pedido {
        value: "1.5".into(),
        ..Pedido::default()
    });
}

#[test]
#[should_panic(expected = "ERC8004_REP_E04: too many decimals")]
fn diecinueve_decimales_es_e04() {
    give_panics(Pedido {
        value_decimals: 19,
        ..Pedido::default()
    });
}

#[test]
#[should_panic(expected = "ERC8004_REP_E04")]
fn decimales_antes_que_el_rango_como_evm() {
    // 1,5e38 entra en i128 y pasa el tope: el que manda es el de los decimales (EVM los mira primero).
    give_panics(Pedido {
        value_decimals: 19,
        value: format!("15{}", "0".repeat(37)),
        ..Pedido::default()
    });
}

#[test]
#[should_panic(expected = "ERC8004_REP_E03: value too large")]
fn value_mayor_a_1e38_es_e03() {
    give_panics(Pedido {
        value: format!("1{}1", "0".repeat(37)),
        ..Pedido::default()
    });
}

#[test]
#[should_panic(expected = "ERC8004_REP_E03")]
fn value_menor_a_menos_1e38_es_e03() {
    give_panics(Pedido {
        value: format!("-1{}1", "0".repeat(37)),
        ..Pedido::default()
    });
}

#[test]
fn los_bordes_de_value_y_decimales_valen() {
    let mut c = new_contract();
    for (i, (v, d)) in [
        (format!("1{}", "0".repeat(38)), 0u8),
        (format!("-1{}", "0".repeat(38)), 18),
        ("0".into(), 18),
        ("-1".into(), 0),
    ]
    .into_iter()
    .enumerate()
    {
        let p = Pedido {
            agent_id: i.to_string(),
            value: v.clone(),
            value_decimals: d,
            ..Pedido::default()
        };
        rate(&mut c, CLIENT, &p);
        let f = c
            .read_feedback(i.to_string(), acc(CLIENT), "1".into())
            .unwrap();
        assert_eq!((f.value, f.value_decimals), (v, d));
    }
}

#[test]
#[should_panic(expected = "ERC8004_REP_E05")]
fn tag1_con_control_es_e05() {
    give_panics(Pedido {
        tag1: Some("a\nb".into()),
        ..Pedido::default()
    });
}

#[test]
#[should_panic(expected = "ERC8004_REP_E05")]
fn tag2_largo_es_e05() {
    give_panics(Pedido {
        tag2: Some("t".repeat(65)),
        ..Pedido::default()
    });
}

#[test]
#[should_panic(expected = "ERC8004_REP_E06")]
fn endpoint_invalido_es_e06() {
    give_panics(Pedido {
        endpoint: Some("GetPrice".into()),
        ..Pedido::default()
    });
}

#[test]
#[should_panic(expected = "ERC8004_REP_E06")]
fn endpoint_largo_es_e06() {
    give_panics(Pedido {
        endpoint: Some(format!("a:{}", "b".repeat(MAX_ENDPOINT_LEN - 1))),
        ..Pedido::default()
    });
}

#[test]
#[should_panic(expected = "ERC8004_REP_E07")]
fn feedback_uri_invalida_es_e07() {
    give_panics(Pedido {
        feedback_uri: Some("ipfs://a b".into()),
        ..Pedido::default()
    });
}

#[test]
#[should_panic(expected = "ERC8004_REP_E07")]
fn feedback_uri_larga_es_e07() {
    give_panics(Pedido {
        feedback_uri: Some(format!("a:{}", "b".repeat(MAX_FEEDBACK_URI_LEN - 1))),
        ..Pedido::default()
    });
}

#[test]
#[should_panic(expected = "ERC8004_REP_E08")]
fn hash_en_mayusculas_es_e08() {
    give_panics(Pedido {
        feedback_hash: Some(HASH.to_uppercase()),
        ..Pedido::default()
    });
}

#[test]
fn los_topes_exactos_valen() {
    let mut c = new_contract();
    let p = Pedido {
        tag1: Some("t".repeat(64)),
        tag2: Some("\u{f1}".repeat(32)),
        endpoint: Some(format!("a:{}", "b".repeat(MAX_ENDPOINT_LEN - 2))),
        feedback_uri: Some(format!("a:{}", "b".repeat(MAX_FEEDBACK_URI_LEN - 2))),
        ..Pedido::default()
    };
    rate(&mut c, CLIENT, &p);
}

#[test]
#[should_panic(expected = "ERC8004_REP_E09: deposito insuficiente para el storage: se necesitan")]
fn un_yocto_menos_de_deposito_es_e09() {
    let mut c = new_contract();
    ctx(CLIENT, ReputationRegistry::feedback_deposit() - 1);
    call_give(&mut c, &Pedido::default());
}

#[test]
#[should_panic(expected = "ERC8004_REP_E10")]
fn gas_de_menos_es_e10() {
    let mut c = new_contract();
    ctx_full(
        CLIENT,
        CLIENT,
        ReputationRegistry::feedback_deposit(),
        HEIGHT,
        MIN_GAS_GIVE_FEEDBACK.saturating_sub(Gas::from_gas(1)),
    );
    call_give(&mut c, &Pedido::default());
}

// Con el gas minimo justo: el mock cobra el gas de otra forma que la wasm; lo prueba el sandbox (WE-09).

#[test]
#[should_panic(expected = "ERC8004_REP_E17")]
fn init_con_el_propio_registro_es_e17() {
    ctx(CONTRACT, 0);
    ReputationRegistry::new(acc(CONTRACT));
}

#[test]
fn revocar_marca_y_deja_de_contar() {
    let mut c = new_contract();
    rate(&mut c, CLIENT, &Pedido::default());
    rate(&mut c, CLIENT, &pedido("7", "10", 0, "starred", ""));
    ctx(CLIENT, 1);
    c.revoke_feedback("7".into(), "1".into());
    assert_eq!(
        events(),
        vec![(
            "feedback_revoked".into(),
            json!({"agent_id": "7", "client_address": CLIENT, "feedback_index": "1"})
        )]
    );
    assert!(
        c.read_feedback("7".into(), acc(CLIENT), "1".into())
            .unwrap()
            .is_revoked
    );
    let s = c.get_summary("7".into(), vec![acc(CLIENT)], None, None);
    assert_eq!((s.count.0, s.summary_value.as_str()), (1, "10"));
}

#[test]
#[should_panic(expected = "ERC8004_REP_E14: Already revoked")]
fn revocar_dos_veces_es_e14() {
    let mut c = new_contract();
    rate(&mut c, CLIENT, &Pedido::default());
    ctx(CLIENT, 1);
    c.revoke_feedback("7".into(), "1".into());
    ctx(CLIENT, 1);
    c.revoke_feedback("7".into(), "1".into());
}

#[test]
#[should_panic(expected = "ERC8004_REP_E12: index must be > 0")]
fn revocar_el_cero_es_e12() {
    let mut c = new_contract();
    rate(&mut c, CLIENT, &Pedido::default());
    ctx(CLIENT, 1);
    c.revoke_feedback("7".into(), "0".into());
}

#[test]
#[should_panic(expected = "ERC8004_REP_E13: index out of bounds")]
fn revocar_mas_alla_del_ultimo_es_e13() {
    let mut c = new_contract();
    rate(&mut c, CLIENT, &Pedido::default());
    ctx(CLIENT, 1);
    c.revoke_feedback("7".into(), "2".into());
}

#[test]
#[should_panic(expected = "ERC8004_REP_E13")]
fn revocar_la_de_otro_cliente_es_e13() {
    let mut c = new_contract();
    rate(&mut c, CLIENT, &Pedido::default());
    ctx(CLIENT2, 1);
    c.revoke_feedback("7".into(), "1".into());
}

#[test]
#[should_panic(expected = "ERC8004_REP_E11")]
fn revocar_indice_no_canonico_es_e11() {
    let mut c = new_contract();
    rate(&mut c, CLIENT, &Pedido::default());
    ctx(CLIENT, 1);
    c.revoke_feedback("7".into(), "01".into());
}

#[test]
#[should_panic(expected = "Requires attached deposit of exactly 1 yoctoNEAR")]
fn revocar_sin_un_yocto() {
    let mut c = new_contract();
    rate(&mut c, CLIENT, &Pedido::default());
    ctx(CLIENT, 0);
    c.revoke_feedback("7".into(), "1".into());
}

#[test]
fn revocar_es_del_predecessor() {
    let mut c = new_contract();
    rate(&mut c, CLIENT, &Pedido::default());
    // Un contrato intermedio firmado por CLIENT revoca como si mismo: no tiene calificaciones.
    ctx_full(OTHER, CLIENT, 1, HEIGHT, Gas::from_tgas(300));
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        c.revoke_feedback("7".into(), "1".into());
    }));
    assert!(r.is_err());
    assert!(
        !c.read_feedback("7".into(), acc(CLIENT), "1".into())
            .unwrap()
            .is_revoked
    );
}
