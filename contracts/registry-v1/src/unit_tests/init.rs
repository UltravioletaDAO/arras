//! `new`: la lista de escrows (E40).

use super::common::*;
use crate::*;

#[test]
fn new_guarda_la_lista_tal_cual() {
    let c = new_contract();
    let cfg = c.get_config();
    assert_eq!(cfg.version, "arras-registry-v1");
    assert_eq!(cfg.escrows, vec![acc(ESCROW), acc(ESCROW2)]);
    assert_eq!(cfg.agents, 0);
    assert_eq!(cfg.feedbacks, 0);
    assert_eq!(cfg.feedback_deposit, U128(deposit()));
    assert_eq!(cfg.min_gas_give_feedback, U64(Gas::from_tgas(60).as_gas()));
    assert_eq!(cfg.limits.max_agent_uri_len, 512);
    assert_eq!(cfg.limits.max_metadata_len, 1024);
    assert_eq!(cfg.limits.max_feedback_uri_len, 512);
    assert_eq!(cfg.limits.max_tag_len, 64);
    assert_eq!(cfg.limits.max_payment_id_len, 64);
    assert_eq!(cfg.limits.max_score, 100);
    assert_eq!(cfg.limits.max_list_limit, 50);
    assert_eq!(cfg.limits.pending_ttl_blocks, U64(1_000));
}

#[test]
fn new_con_ocho_escrows() {
    let nombres: Vec<String> = (0..8).map(|i| format!("e{i}.testnet")).collect();
    let refs: Vec<&str> = nombres.iter().map(|s| s.as_str()).collect();
    let c = new_with(&refs);
    assert_eq!(c.get_config().escrows.len(), 8);
}

#[test]
#[should_panic(expected = "ARRAS_REG_E40")]
fn new_sin_escrows() {
    new_with(&[]);
}

#[test]
#[should_panic(expected = "ARRAS_REG_E40")]
fn new_con_nueve_escrows() {
    let nombres: Vec<String> = (0..9).map(|i| format!("e{i}.testnet")).collect();
    let refs: Vec<&str> = nombres.iter().map(|s| s.as_str()).collect();
    new_with(&refs);
}

#[test]
#[should_panic(expected = "ARRAS_REG_E40")]
fn new_con_escrow_repetido() {
    new_with(&[ESCROW, ESCROW2, ESCROW]);
}

#[test]
#[should_panic(expected = "ARRAS_REG_E40")]
fn new_con_el_propio_registro() {
    new_with(&[ESCROW, CONTRACT]);
}
