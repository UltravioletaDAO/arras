//! `docs/vectores.json`: los vectores de v2 se leen del archivo (lo que usan los SDK) y se recomputan con el codigo
//! del contrato; los de v1 (V-1..V-3) dan el mismo `payment_id` con el parser de v2.

use crate::msg::{FtMsg, OperatorTicket, parse_authorize_msg_v2, parse_ft_msg};
use crate::ticket::{TicketSubject, ticket_message, verify_ticket};
use arras_escrow_core::{PaymentIdInput, compute_payment_id};
use near_sdk::serde_json::{self, Value};

fn vectores() -> Value {
    let p = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/vectores.json");
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

fn s(v: &Value) -> &str {
    v.as_str().unwrap()
}

#[test]
fn vectores_v1_valen_en_v2() {
    let j = vectores();
    for v in j["payment_id"]["vectores"].as_array().unwrap() {
        let msg = format!(
            "{{\"v\":1,\"action\":\"authorize\",\"operator_id\":\"{}\",\"receiver\":\"{}\",\"release_deadline\":\"{}\",\"reclaim_after\":\"{}\",\"fee_bps_max\":{},\"terms_hash\":\"{}\",\"salt\":\"{}\"}}",
            s(&v["operator_id"]),
            s(&v["receiver"]),
            s(&v["release_deadline"]),
            s(&v["reclaim_after"]),
            v["fee_bps_max"],
            s(&v["terms_hash"]),
            s(&v["salt"])
        );
        let m = parse_authorize_msg_v2(&msg).unwrap();
        assert_eq!(m.to_canonical_json(), msg);
        let id = compute_payment_id(&PaymentIdInput::from_msg(
            &m.base,
            s(&v["token"]),
            s(&v["payer"]),
            s(&v["amount"]).parse().unwrap(),
        ))
        .1;
        assert_eq!(id, s(&v["payment_id"]), "{}", v["id"]);
    }
}

#[test]
fn vectores_v2_del_json() {
    let j = vectores();
    let v2 = &j["v2"];
    let usdc = "3e2210e1184b45b64c8a434c0a7e7b23cc04ea7eb7a6c3c32520d03d4afcb8af";
    // msg authorize: los tres canonicos parsean, vuelven iguales y dan el mismo id (el de V-1).
    for k in [
        "canonico_v1_en_v2",
        "canonico_con_dispute_deadline",
        "canonico_con_ticket_t1",
    ] {
        let msg = s(&v2["msg_authorize"][k]);
        let m = parse_authorize_msg_v2(msg).unwrap();
        assert_eq!(m.to_canonical_json(), msg, "{k}");
        let id = compute_payment_id(&PaymentIdInput::from_msg(
            &m.base,
            usdc,
            "payer-demo.testnet",
            1_000_000,
        ))
        .1;
        assert_eq!(id, s(&v2["msg_authorize"]["payment_id_de_los_tres"]), "{k}");
    }
    assert_eq!(
        s(&v2["msg_authorize"]["canonico_v1_en_v2"]),
        s(&j["msg_canonico_v1"])
    );
    // msg budget.
    let b = s(&v2["msg_budget"]["canonico"]);
    match parse_ft_msg(b).unwrap() {
        FtMsg::Budget(m) => assert_eq!(m.to_canonical_json(), b),
        FtMsg::Authorize(_) => panic!("es un budget"),
    }
    // VB-1.
    for v in v2["authorize_from_budget"]["vectores"].as_array().unwrap() {
        let m = arras_escrow_core::AuthorizeMsg {
            operator_id: s(&v["operator_id"]).into(),
            receiver: s(&v["receiver"]).parse().unwrap(),
            release_deadline: s(&v["release_deadline"]).parse().unwrap(),
            reclaim_after: s(&v["reclaim_after"]).parse().unwrap(),
            fee_bps_max: v["fee_bps_max"].as_u64().unwrap() as u16,
            terms_hash: crate::msg::parse_hex_32(s(&v["terms_hash"])).unwrap(),
            salt: crate::msg::parse_hex_16(s(&v["salt"])).unwrap(),
        };
        let id = compute_payment_id(&PaymentIdInput::from_msg(
            &m,
            s(&v["token"]),
            s(&v["owner"]),
            s(&v["amount"]).parse().unwrap(),
        ))
        .1;
        assert_eq!(id, s(&v["payment_id"]), "{}", v["id"]);
    }
    // Tickets T-1 y T-2: mensaje y firma exactos, y la llave publica en los dos formatos.
    let t = &v2["operator_ticket"];
    let pk = crate::msg::parse_hex_32(s(&t["llave_de_prueba"]["pubkey_hex"])).unwrap();
    let llave: near_sdk::PublicKey = s(&t["llave_de_prueba"]["ticket_pubkey"]).parse().unwrap();
    assert_eq!(&llave.as_bytes()[1..], &pk);
    assert_eq!(
        ed25519_dalek::SigningKey::from_bytes(
            &crate::msg::parse_hex_32(s(&t["llave_de_prueba"]["semilla_hex"])).unwrap()
        )
        .verifying_key()
        .to_bytes(),
        pk
    );
    for v in t["vectores"].as_array().unwrap() {
        let contract = s(&v["contract"]);
        let expiry: u64 = s(&v["expiry"]).parse().unwrap();
        let sub = &v["subject"];
        let subject = if let Some(p) = sub.get("payment") {
            TicketSubject::Payment {
                payment_id: s(&p["payment_id"]),
            }
        } else {
            let b = &sub["budget"];
            TicketSubject::Budget {
                operator_id: s(&b["operator_id"]),
                owner: s(&b["owner"]),
                agent: s(&b["agent"]),
            }
        };
        let m = ticket_message(contract, subject, expiry);
        assert_eq!(
            crate::msg::hex_encode(&m),
            s(&v["mensaje_hex"]),
            "{}",
            v["id"]
        );
        let ticket = OperatorTicket {
            expiry,
            sig: s(&v["sig"]).into(),
        };
        assert!(
            verify_ticket(&pk, contract, subject, &ticket),
            "{}",
            v["id"]
        );
    }
    // Lo demas que el JSON declara, contra las constantes del contrato.
    assert_eq!(
        v2["disputa"]["storage_reservado_bytes"].as_u64(),
        Some(crate::DISPUTE_RESERVE_BYTES)
    );
    assert_eq!(
        s(&v2["disputa"]["plazo_del_arbitro_ns"]["min"]),
        crate::MIN_ARBITER_WINDOW_NS.to_string()
    );
    assert_eq!(
        s(&v2["disputa"]["plazo_del_arbitro_ns"]["max"]),
        crate::MAX_ARBITER_WINDOW_NS.to_string()
    );
    assert_eq!(
        s(&t["mensaje"]).contains(crate::ticket::TICKET_DOMAIN),
        true
    );
    let codigos: Vec<&str> = v2["codigos_nuevos"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap())
        .collect();
    use crate::error::ArrasErrorV2 as E;
    let todos = [
        E::E47VentanaArbitro,
        E::E48TicketPubkey,
        E::E50SinPresupuesto,
        E::E51TopeDiario,
        E::E52TopePorPagoPresupuesto,
        E::E53TicketInvalido,
        E::E54TicketVencido,
        E::E55AgenteInvalido,
        E::E56TopesPresupuesto,
        E::E57PlazoPresupuesto,
        E::E60NoEsParte,
        E::E61FueraDePlazoDisputa,
        E::E62NoEsArbitro,
        E::E63RepartoInvalido,
        E::E64ArbitroEnPlazo,
        E::E65ArbitroVencido,
        E::E66ReasonHash,
    ];
    assert_eq!(codigos, todos.iter().map(|e| e.code()).collect::<Vec<_>>());
}
