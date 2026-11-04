//! U-ST: `FEEDBACK_MAX_BYTES` (y `FEEDBACK_PROOF_MAX_BYTES`) son exactamente el peor caso medido, y el peor caso de logs
//! de `new_feedback` entra en los 16.384 bytes de NEAR.

use super::common::*;
use crate::*;
use near_sdk::test_utils::get_logs;

fn al_tope() -> Pedido {
    Pedido {
        agent_id: u64::MAX.to_string(),
        value: format!("-1{}", "0".repeat(38)),
        value_decimals: 18,
        // `"` y `\` se escapan al doble en el log: el peor caso de bytes de log.
        tag1: Some("\"".repeat(64)),
        tag2: Some("\\".repeat(64)),
        endpoint: Some(format!("a:{}", "b".repeat(MAX_ENDPOINT_LEN - 2))),
        feedback_uri: Some(format!("a:{}", "b".repeat(MAX_FEEDBACK_URI_LEN - 2))),
        feedback_hash: Some(HASH.into()),
    }
}

#[test]
fn ust1_peor_caso_de_una_calificacion() {
    let client = "c".repeat(64);
    let mut c = new_contract();
    let p = give(&mut c, &client, &al_tope());
    ctx_callback(vec![agent_ok()], HEIGHT + 2);
    c.flush_all();
    let antes = env::storage_usage();
    assert!(c.on_feedback_checked(p).is_some(), "{:?}", get_logs());
    c.flush_all();
    assert_eq!(env::storage_usage() - antes, FEEDBACK_MAX_BYTES);
    assert_eq!(FEEDBACK_MAX_BYTES, 591);
    assert_eq!(
        transfers(),
        vec![(client.clone(), 0)]
            .into_iter()
            .filter(|t| t.1 > 0)
            .collect::<Vec<_>>()
    );
    let total: usize = get_logs().iter().map(String::len).sum();
    assert!(total < 16_384, "logs: {total}");
}

#[test]
fn ust2_la_segunda_del_mismo_cliente_no_paga_el_cliente_nuevo() {
    let mut c = new_contract();
    rate(&mut c, CLIENT, &Pedido::default());
    let p = give(&mut c, CLIENT, &Pedido::default());
    ctx_callback(vec![agent_ok()], HEIGHT + 2);
    c.flush_all();
    let antes = env::storage_usage();
    c.on_feedback_checked(p).unwrap();
    c.flush_all();
    let segunda = env::storage_usage() - antes;
    let client_bytes = CLIENT.len() as u64;
    // La Feedback y su last_index (sin el Vector del agente ni su elemento).
    let key = 1 + 8 + 4 + client_bytes + 8;
    let esperado = erc8004_common::storage::record_bytes(key, 16 + 1 + 1 + 4 + 7 + 4)
        + erc8004_common::storage::record_bytes(1 + 8 + 4 + client_bytes, 8)
        - erc8004_common::storage::record_bytes(1 + 8 + 4 + client_bytes, 8);
    assert_eq!(segunda, esperado);
}
