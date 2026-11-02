//! Los `msg` de `ft_transfer_call` que acepta v2 (prescripcion sec. 4):
//!
//! - `authorize`: el `msg` de v1 (sec. 3.2) **mas dos campos opcionales**, en este orden al final:
//!   `dispute_deadline` (U64 en string; si falta, vale `reclaim_after`) y `operator_ticket`
//!   (`{"expiry":"<ns>","sig":"<base58 de 64 bytes>"}`). Un `msg` de v1 es un `msg` valido de v2: el SDK de v1 sirve
//!   tal cual y el `payment_id` es el mismo (la preimagen de la sec. 7 no cambia; los campos nuevos no entran).
//! - `budget`: `{"v":1,"action":"budget","operator_id":"em","agent":"<cuenta>","tope_por_pago":"2000000",
//!   "tope_diario":"10000000"}` mas `operator_ticket` opcional al final.
//!
//! Igual que v1: `deny_unknown_fields`, campo repetido o desconocido = `E03`, hex en minusculas y con el largo exacto.
//! El contrato no exige el orden canonico; el SDK y el facilitador comparan el canonico byte a byte.

use near_sdk::json_types::{U64, U128};
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{AccountId, env, serde_json};

use crate::error::ArrasErrorV2;
use arras_escrow_core::{ArrasError, AuthorizeMsg, MSG_VERSION};

/// `operator_ticket` ya parseado. La firma queda en base58 sin decodificar: un `sig` mal formado es un ticket
/// invalido (`E53`), no un `msg` invalido (`E03`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperatorTicket {
    pub expiry: u64,
    pub sig: String,
}

#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde", deny_unknown_fields)]
struct TicketJson {
    expiry: U64,
    sig: String,
}

impl From<TicketJson> for OperatorTicket {
    fn from(t: TicketJson) -> Self {
        Self {
            expiry: t.expiry.0,
            sig: t.sig,
        }
    }
}

impl From<&OperatorTicket> for TicketJson {
    fn from(t: &OperatorTicket) -> Self {
        Self {
            expiry: U64(t.expiry),
            sig: t.sig.clone(),
        }
    }
}

/// El `msg` `authorize` de v2 tal como llega. Las claves del canonico van en este orden.
#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde", deny_unknown_fields)]
struct AuthorizeMsgV2Json {
    v: u8,
    action: String,
    operator_id: String,
    receiver: AccountId,
    release_deadline: U64,
    reclaim_after: U64,
    fee_bps_max: u16,
    terms_hash: String,
    salt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    dispute_deadline: Option<U64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    operator_ticket: Option<TicketJson>,
}

/// El `msg` `authorize` de v2 ya validado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizeMsgV2 {
    /// Los campos de v1: con ellos sale el `payment_id` ([`crate::PaymentIdInput::from_msg`]).
    pub base: AuthorizeMsg,
    pub dispute_deadline: Option<u64>,
    pub operator_ticket: Option<OperatorTicket>,
}

impl AuthorizeMsgV2 {
    /// `dispute_deadline`, o `reclaim_after` si el `msg` no lo trae.
    pub fn effective_dispute_deadline(&self) -> u64 {
        self.dispute_deadline.unwrap_or(self.base.reclaim_after)
    }

    /// El JSON canonico: las claves de v1 en su orden y despues, si estan, `dispute_deadline` y `operator_ticket`.
    pub fn to_canonical_json(&self) -> String {
        let b = &self.base;
        let json = AuthorizeMsgV2Json {
            v: MSG_VERSION,
            action: "authorize".into(),
            operator_id: b.operator_id.clone(),
            receiver: b.receiver.clone(),
            release_deadline: U64(b.release_deadline),
            reclaim_after: U64(b.reclaim_after),
            fee_bps_max: b.fee_bps_max,
            terms_hash: hex_encode(&b.terms_hash),
            salt: hex_encode(&b.salt),
            dispute_deadline: self.dispute_deadline.map(U64),
            operator_ticket: self.operator_ticket.as_ref().map(TicketJson::from),
        };
        serde_json::to_string(&json).unwrap_or_else(|_| env::abort())
    }
}

/// Parsea un `msg` `authorize` de v2. Cualquier falla es `E03_MSG_INVALIDO`.
pub fn parse_authorize_msg_v2(msg: &str) -> Result<AuthorizeMsgV2, ArrasErrorV2> {
    let json: AuthorizeMsgV2Json =
        serde_json::from_str(msg).map_err(|_| ArrasError::E03MsgInvalido)?;
    if json.v != MSG_VERSION || json.action != "authorize" {
        return Err(ArrasError::E03MsgInvalido.into());
    }
    let terms_hash = hex_decode::<32>(&json.terms_hash).ok_or(ArrasError::E03MsgInvalido)?;
    let salt = hex_decode::<16>(&json.salt).ok_or(ArrasError::E03MsgInvalido)?;
    Ok(AuthorizeMsgV2 {
        base: AuthorizeMsg {
            operator_id: json.operator_id,
            receiver: json.receiver,
            release_deadline: json.release_deadline.0,
            reclaim_after: json.reclaim_after.0,
            fee_bps_max: json.fee_bps_max,
            terms_hash,
            salt,
        },
        dispute_deadline: json.dispute_deadline.map(|d| d.0),
        operator_ticket: json.operator_ticket.map(OperatorTicket::from),
    })
}

#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde", deny_unknown_fields)]
struct BudgetMsgJson {
    v: u8,
    action: String,
    operator_id: String,
    agent: AccountId,
    tope_por_pago: U128,
    tope_diario: U128,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    operator_ticket: Option<TicketJson>,
}

/// El `msg` `budget`: deposita en el presupuesto `(sender_id, agent, operator_id)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BudgetMsg {
    pub operator_id: String,
    pub agent: AccountId,
    pub tope_por_pago: u128,
    pub tope_diario: u128,
    pub operator_ticket: Option<OperatorTicket>,
}

impl BudgetMsg {
    /// El JSON canonico de la sec. 4 (con `operator_ticket` al final si esta).
    pub fn to_canonical_json(&self) -> String {
        let json = BudgetMsgJson {
            v: MSG_VERSION,
            action: "budget".into(),
            operator_id: self.operator_id.clone(),
            agent: self.agent.clone(),
            tope_por_pago: U128(self.tope_por_pago),
            tope_diario: U128(self.tope_diario),
            operator_ticket: self.operator_ticket.as_ref().map(TicketJson::from),
        };
        serde_json::to_string(&json).unwrap_or_else(|_| env::abort())
    }
}

/// Parsea un `msg` `budget`. Cualquier falla es `E03_MSG_INVALIDO` (los topes en 0 se rechazan despues, con `E56`).
pub fn parse_budget_msg(msg: &str) -> Result<BudgetMsg, ArrasErrorV2> {
    let json: BudgetMsgJson = serde_json::from_str(msg).map_err(|_| ArrasError::E03MsgInvalido)?;
    if json.v != MSG_VERSION || json.action != "budget" {
        return Err(ArrasError::E03MsgInvalido.into());
    }
    Ok(BudgetMsg {
        operator_id: json.operator_id,
        agent: json.agent,
        tope_por_pago: json.tope_por_pago.0,
        tope_diario: json.tope_diario.0,
        operator_ticket: json.operator_ticket.map(OperatorTicket::from),
    })
}

/// Lo que puede llegar en el `msg` de `ft_transfer_call` a v2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FtMsg {
    Authorize(AuthorizeMsgV2),
    Budget(BudgetMsg),
}

/// `authorize` o `budget`; si no es ninguno de los dos, `E03`.
pub fn parse_ft_msg(msg: &str) -> Result<FtMsg, ArrasErrorV2> {
    if let Ok(a) = parse_authorize_msg_v2(msg) {
        return Ok(FtMsg::Authorize(a));
    }
    parse_budget_msg(msg).map(FtMsg::Budget)
}

const HEX: &[u8; 16] = b"0123456789abcdef";

/// Hex en minusculas (copia de la de `arras-escrow-core`, que es privada y no se toca).
pub(crate) fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(HEX[usize::from(b >> 4)] as char);
        s.push(HEX[usize::from(b & 0x0f)] as char);
    }
    s
}

fn hex_nibble(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        _ => None,
    }
}

/// Hex en minusculas, exactamente `2 * N` caracteres (la misma regla que el `msg` de v1).
pub(crate) fn hex_decode<const N: usize>(s: &str) -> Option<[u8; N]> {
    let b = s.as_bytes();
    if b.len() != 2 * N {
        return None;
    }
    let mut out = [0u8; N];
    for (i, o) in out.iter_mut().enumerate() {
        *o = (hex_nibble(b[2 * i])? << 4) | hex_nibble(b[2 * i + 1])?;
    }
    Some(out)
}

/// Hex de 64 en minusculas -> 32 bytes (`terms_hash`, `reason_hash`).
pub fn parse_hex_32(s: &str) -> Option<[u8; 32]> {
    hex_decode::<32>(s)
}

/// Hex de 32 en minusculas -> 16 bytes (`salt`).
pub fn parse_hex_16(s: &str) -> Option<[u8; 16]> {
    hex_decode::<16>(s)
}

/// 32 bytes -> hex de 64 en minusculas.
pub fn hex_encode_32(b: &[u8; 32]) -> String {
    hex_encode(b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use arras_escrow_core::{PaymentIdInput, compute_payment_id, parse_authorize_msg};

    const USDC_TESTNET: &str = "3e2210e1184b45b64c8a434c0a7e7b23cc04ea7eb7a6c3c32520d03d4afcb8af";
    const RELEASE: u64 = 1_793_577_600_000_000_000;
    const RECLAIM: u64 = 1_793_664_000_000_000_000;

    fn msg_v1() -> String {
        format!(
            "{{\"v\":1,\"action\":\"authorize\",\"operator_id\":\"em\",\"receiver\":\"worker-demo.testnet\",\"release_deadline\":\"{RELEASE}\",\"reclaim_after\":\"{RECLAIM}\",\"fee_bps_max\":1300,\"terms_hash\":\"{}\",\"salt\":\"{}\"}}",
            "0".repeat(64),
            "0".repeat(32)
        )
    }

    fn msg_v2_completo() -> String {
        format!(
            "{},\"dispute_deadline\":\"{RELEASE}\",\"operator_ticket\":{{\"expiry\":\"{RELEASE}\",\"sig\":\"abc\"}}}}",
            msg_v1().trim_end_matches('}')
        )
    }

    #[test]
    fn msg_de_v1_es_msg_de_v2_con_el_mismo_payment_id() {
        let v2 = parse_authorize_msg_v2(&msg_v1()).unwrap();
        assert_eq!(v2.dispute_deadline, None);
        assert_eq!(v2.operator_ticket, None);
        assert_eq!(v2.effective_dispute_deadline(), RECLAIM);
        assert_eq!(v2.to_canonical_json(), msg_v1());
        assert_eq!(v2.base, parse_authorize_msg(&msg_v1()).unwrap());
        let id = compute_payment_id(&PaymentIdInput::from_msg(
            &v2.base,
            USDC_TESTNET,
            "payer-demo.testnet",
            1_000_000,
        ));
        assert_eq!(id.1, "DN5eMA3sNES7xsoy6qvy24cVF6Dmwjxpkvd2b4e8P38M", "V-1");
    }

    #[test]
    fn msg_v2_completo_ida_y_vuelta() {
        let m = parse_authorize_msg_v2(&msg_v2_completo()).unwrap();
        assert_eq!(m.dispute_deadline, Some(RELEASE));
        assert_eq!(m.effective_dispute_deadline(), RELEASE);
        assert_eq!(
            m.operator_ticket,
            Some(OperatorTicket {
                expiry: RELEASE,
                sig: "abc".into()
            })
        );
        assert_eq!(m.to_canonical_json(), msg_v2_completo());
        // v1 no acepta el msg de v2 (deny_unknown_fields): un msg de v2 mandado a v1 vuelve entero con E03.
        assert_eq!(
            parse_authorize_msg(&msg_v2_completo()),
            Err(ArrasError::E03MsgInvalido)
        );
    }

    #[test]
    fn msg_v2_invalido_es_e03() {
        let base = msg_v2_completo();
        let malos = [
            String::new(),
            "{}".into(),
            base.replace("\"v\":1", "\"v\":2"),
            base.replace("\"action\":\"authorize\"", "\"action\":\"budget\""),
            base.replace(
                &format!("\"dispute_deadline\":\"{RELEASE}\""),
                &format!("\"dispute_deadline\":{RELEASE}"),
            ),
            base.replace("\"sig\":\"abc\"", "\"sig\":\"abc\",\"x\":1"),
            base.replace(",\"sig\":\"abc\"", ""),
            base.replace(&format!("\"expiry\":\"{RELEASE}\","), ""),
            base.replace(
                &format!("\"expiry\":\"{RELEASE}\""),
                &format!("\"expiry\":{RELEASE}"),
            ),
            base.replace("\"sig\":\"abc\"", "\"sig\":1"),
            base.replace("\"operator_ticket\":{", "\"operator_ticket\":[{")
                .replace("}}", "}]}"),
            base.replace("}}", "},\"extra\":1}"),
            base.replace(
                "\"dispute_deadline\"",
                "\"dispute_deadline\":\"1\",\"dispute_deadline\"",
            ),
            base.replace(&"0".repeat(64), &"A".repeat(64)),
            base.replace(&"0".repeat(32), &"0".repeat(30)),
            format!("{base}x"),
        ];
        for m in malos {
            assert_eq!(
                parse_authorize_msg_v2(&m),
                Err(ArrasError::E03MsgInvalido.into()),
                "{m}"
            );
        }
    }

    fn budget_msg() -> String {
        "{\"v\":1,\"action\":\"budget\",\"operator_id\":\"em\",\"agent\":\"agent-demo.testnet\",\"tope_por_pago\":\"2000000\",\"tope_diario\":\"10000000\"}".into()
    }

    #[test]
    fn msg_budget_canonico_de_la_sec_4() {
        let b = parse_budget_msg(&budget_msg()).unwrap();
        assert_eq!(
            b,
            BudgetMsg {
                operator_id: "em".into(),
                agent: "agent-demo.testnet".parse().unwrap(),
                tope_por_pago: 2_000_000,
                tope_diario: 10_000_000,
                operator_ticket: None,
            }
        );
        assert_eq!(b.to_canonical_json(), budget_msg());
        let con_ticket = budget_msg().replace(
            "\"}",
            &format!("\",\"operator_ticket\":{{\"expiry\":\"{RELEASE}\",\"sig\":\"xyz\"}}}}"),
        );
        let t = parse_budget_msg(&con_ticket).unwrap();
        assert_eq!(t.operator_ticket.as_ref().unwrap().sig, "xyz");
        assert_eq!(t.to_canonical_json(), con_ticket);
    }

    #[test]
    fn msg_budget_invalido_es_e03() {
        let base = budget_msg();
        let malos = [
            base.replace("\"v\":1", "\"v\":2"),
            base.replace("\"action\":\"budget\"", "\"action\":\"authorize\""),
            base.replace("\"tope_por_pago\":\"2000000\"", "\"tope_por_pago\":2000000"),
            base.replace("\"tope_diario\":\"10000000\"", "\"tope_diario\":\"-1\""),
            base.replace("agent-demo.testnet", "Agent.testnet"),
            base.replace(",\"agent\":\"agent-demo.testnet\"", ""),
            base.replace("}", ",\"receiver\":\"x.testnet\"}"),
            base.replace("\"v\":1,", "\"v\":1,\"v\":1,"),
        ];
        for m in malos {
            assert_eq!(
                parse_budget_msg(&m),
                Err(ArrasError::E03MsgInvalido.into()),
                "{m}"
            );
        }
    }

    #[test]
    fn parse_ft_msg_despacha_por_action() {
        assert!(matches!(parse_ft_msg(&msg_v1()), Ok(FtMsg::Authorize(_))));
        assert!(matches!(
            parse_ft_msg(&msg_v2_completo()),
            Ok(FtMsg::Authorize(_))
        ));
        assert!(matches!(parse_ft_msg(&budget_msg()), Ok(FtMsg::Budget(_))));
        for malo in ["", "{}", "{\"v\":1,\"action\":\"refund\"}"] {
            assert_eq!(parse_ft_msg(malo), Err(ArrasError::E03MsgInvalido.into()));
        }
    }

    #[test]
    fn hex_publico() {
        assert_eq!(parse_hex_32(&"ab".repeat(32)), Some([0xab; 32]));
        assert_eq!(parse_hex_32(&"AB".repeat(32)), None);
        assert_eq!(parse_hex_16(&"0".repeat(32)), Some([0; 16]));
        assert_eq!(parse_hex_16(&"0".repeat(31)), None);
        assert_eq!(hex_encode_32(&[0xab; 32]), "ab".repeat(32));
    }
}
