//! `set_agent_wallet`, `accept_agent_wallet` y `unset_agent_wallet` (desviacion I-1: la wallet confirma llamando).

use super::common::*;
use crate::*;
use near_sdk::serde_json::json;

const MIN5: u64 = MAX_DEADLINE_DELAY_NS;

fn d(ns: u64) -> String {
    ns.to_string()
}

fn wallet_event(v: &str) -> (String, String, near_sdk::serde_json::Value) {
    (
        "erc8004".into(),
        "metadata_set".into(),
        json!({"agent_id": "0", "metadata_key": "agentWallet", "metadata_value": b64(v.as_bytes())}),
    )
}

#[test]
fn el_dueno_se_pone_a_si_mismo_en_el_acto() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    transfer(&mut c, ALICE, BOB, "0");
    assert_eq!(c.get_agent_wallet("0".into()), None);
    ctx(BOB, 1);
    assert!(c.set_agent_wallet("0".into(), acc(BOB), d(NOW + MIN5)));
    assert_eq!(events(), vec![wallet_event(BOB)]);
    assert_eq!(c.get_agent_wallet("0".into()), Some(acc(BOB)));
    assert_eq!(
        c.get_metadata("0".into(), "agentWallet".into()).unwrap().0,
        BOB.as_bytes()
    );
}

#[test]
fn otra_wallet_queda_pendiente_hasta_que_acepta() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    ctx(ALICE, 1);
    assert!(!c.set_agent_wallet("0".into(), acc(WALLET), d(NOW + 10)));
    assert_eq!(events(), vec![]);
    assert_eq!(c.get_agent_wallet("0".into()), Some(acc(ALICE)));
    let p = c.get_pending_agent_wallet("0".into()).unwrap();
    assert_eq!((p.wallet, p.deadline.0), (acc(WALLET), NOW + 10));
    ctx_at(WALLET, 1, NOW + 10);
    c.accept_agent_wallet("0".into());
    assert_eq!(events(), vec![wallet_event(WALLET)]);
    assert_eq!(c.get_agent_wallet("0".into()), Some(acc(WALLET)));
    assert_eq!(c.get_pending_agent_wallet("0".into()), None);
}

#[test]
fn un_aprobado_tambien_y_una_nueva_pisa_la_vieja() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    approve(&mut c, ALICE, "0", OP);
    ctx(OP, 1);
    assert!(!c.set_agent_wallet("0".into(), acc(WALLET), d(NOW + 10)));
    ctx(OP, 1);
    assert!(!c.set_agent_wallet("0".into(), acc(CAROL), d(NOW + 20)));
    assert_eq!(
        c.get_pending_agent_wallet("0".into()).unwrap().wallet,
        acc(CAROL)
    );
    // El aprobado se pone a si mismo: en el acto, y borra la pendiente.
    ctx(OP, 1);
    assert!(c.set_agent_wallet("0".into(), acc(OP), d(NOW)));
    assert_eq!(c.get_agent_wallet("0".into()), Some(acc(OP)));
    assert_eq!(c.get_pending_agent_wallet("0".into()), None);
}

#[test]
#[should_panic(expected = "ERC8004_ID_E12: Not authorized")]
fn set_agent_wallet_de_otro_es_e12() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    ctx(BOB, 1);
    c.set_agent_wallet("0".into(), acc(BOB), d(NOW));
}

#[test]
#[should_panic(expected = "Requires attached deposit of exactly 1 yoctoNEAR")]
fn set_agent_wallet_sin_un_yocto_exacto() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    ctx(ALICE, 2);
    c.set_agent_wallet("0".into(), acc(ALICE), d(NOW));
}

#[test]
#[should_panic(expected = "ERC8004_ID_E13")]
fn deadline_no_canonico_es_e13() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    ctx(ALICE, 1);
    c.set_agent_wallet("0".into(), acc(ALICE), format!("0{NOW}"));
}

#[test]
#[should_panic(expected = "ERC8004_ID_E14: expired")]
fn deadline_vencido_es_e14() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    ctx(ALICE, 1);
    c.set_agent_wallet("0".into(), acc(ALICE), d(NOW - 1));
}

#[test]
fn deadline_justo_en_now_y_en_now_mas_5_min_vale() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    ctx(ALICE, 1);
    assert!(c.set_agent_wallet("0".into(), acc(ALICE), d(NOW)));
    ctx(ALICE, 1);
    assert!(!c.set_agent_wallet("0".into(), acc(WALLET), d(NOW + MIN5)));
}

#[test]
#[should_panic(expected = "ERC8004_ID_E15: deadline too far")]
fn deadline_mas_alla_de_5_min_es_e15() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    ctx(ALICE, 1);
    c.set_agent_wallet("0".into(), acc(ALICE), d(NOW + MIN5 + 1));
}

#[test]
#[should_panic(expected = "ERC8004_ID_E16")]
fn aceptar_sin_pendiente_es_e16() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    ctx(WALLET, 1);
    c.accept_agent_wallet("0".into());
}

#[test]
#[should_panic(expected = "ERC8004_ID_E17")]
fn aceptar_otra_cuenta_es_e17() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    ctx(ALICE, 1);
    c.set_agent_wallet("0".into(), acc(WALLET), d(NOW + 10));
    // Ni siquiera el dueno puede aceptar por la wallet.
    ctx(ALICE, 1);
    c.accept_agent_wallet("0".into());
}

#[test]
#[should_panic(expected = "ERC8004_ID_E14")]
fn aceptar_despues_del_deadline_es_e14() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    ctx(ALICE, 1);
    c.set_agent_wallet("0".into(), acc(WALLET), d(NOW + 10));
    ctx_at(WALLET, 1, NOW + 11);
    c.accept_agent_wallet("0".into());
}

#[test]
#[should_panic(expected = "Requires attached deposit of exactly 1 yoctoNEAR")]
fn aceptar_sin_un_yocto() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    ctx(ALICE, 1);
    c.set_agent_wallet("0".into(), acc(WALLET), d(NOW + 10));
    ctx(WALLET, 0);
    c.accept_agent_wallet("0".into());
}

#[test]
#[should_panic(expected = "ERC8004_ID_E16")]
fn la_transferencia_borra_la_pendiente() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    ctx(ALICE, 1);
    c.set_agent_wallet("0".into(), acc(WALLET), d(NOW + 10));
    transfer(&mut c, ALICE, BOB, "0");
    assert_eq!(c.get_pending_agent_wallet("0".into()), None);
    ctx(WALLET, 1);
    c.accept_agent_wallet("0".into());
}

#[test]
fn unset_borra_la_wallet_y_la_pendiente() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    ctx(ALICE, 1);
    c.set_agent_wallet("0".into(), acc(WALLET), d(NOW + 10));
    approve(&mut c, ALICE, "0", OP);
    ctx(OP, 1);
    c.unset_agent_wallet("0".into());
    assert_eq!(events(), vec![wallet_event("")]);
    assert_eq!(c.get_agent_wallet("0".into()), None);
    assert_eq!(c.get_metadata("0".into(), "agentWallet".into()), None);
    assert_eq!(c.get_pending_agent_wallet("0".into()), None);
}

#[test]
#[should_panic(expected = "ERC8004_ID_E12")]
fn unset_de_otro_es_e12() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    ctx(WALLET, 1);
    c.unset_agent_wallet("0".into());
}

#[test]
#[should_panic(expected = "Requires attached deposit of exactly 1 yoctoNEAR")]
fn unset_sin_un_yocto() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    ctx(ALICE, 0);
    c.unset_agent_wallet("0".into());
}

#[test]
fn la_wallet_no_cambia_el_storage() {
    let mut c = new_contract();
    reg(&mut c, ALICE);
    let wallet64 = "w".repeat(64);
    ctx(ALICE, 1);
    c.flush_all();
    let antes = env::storage_usage();
    c.set_agent_wallet("0".into(), acc(&wallet64), d(NOW + 10));
    c.flush_all();
    assert_eq!(env::storage_usage(), antes);
    ctx(&wallet64, 1);
    c.flush_all();
    let antes = env::storage_usage();
    c.accept_agent_wallet("0".into());
    c.flush_all();
    assert_eq!(env::storage_usage(), antes);
    assert_eq!(transfers(), vec![]);
}
