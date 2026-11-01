//! El `msg` de `ft_transfer_call` (sec. 3.2) y el `payment_id` (sec. 7).
//!
//! `payment_id = base58(sha256(borsh(preimagen)))`; preimagen en este orden: `v` u8 (=1), `operator_id` String,
//! `token` String, `payer` String, `receiver` String, `amount` u128, `release_deadline` u64, `reclaim_after` u64,
//! `fee_bps_max` u16, `terms_hash` [u8;32], `salt` [u8;16] (String = u32 LE + bytes; enteros LE).

use near_sdk::borsh::{self, BorshSerialize};
use near_sdk::json_types::U64;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{AccountId, bs58, env, serde_json};

use crate::ArrasError;

/// String base58 de un hash de 32 bytes: **de 32 a 44 caracteres**. Casi siempre 43 o 44 (V-2 tiene 43), pero un hash
/// con bytes altos chicos da 42 o menos (p. ej. `1y1ZbrieUbaioiUBHxX7EyZMNkCis2tLKCs5Xxezgy`, salt `00..0415` sobre V-1;
/// refutacion de H-1, P2-1): nadie puede asumir un largo fijo.
pub type PaymentId = String;

/// `v` del `msg` y de la preimagen.
pub const MSG_VERSION: u8 = 1;

/// Largo maximo de un `operator_id` (`[a-z0-9_-]{1,32}`).
pub const MAX_OPERATOR_ID_LEN: usize = 32;

/// `[a-z0-9_-]{1,32}`.
pub fn is_valid_operator_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_OPERATOR_ID_LEN
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

/// El `msg` tal como llega, con `deny_unknown_fields`. Las claves del canonico van en este orden.
#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde", deny_unknown_fields)]
struct AuthorizeMsgJson {
    v: u8,
    action: String,
    operator_id: String,
    receiver: AccountId,
    release_deadline: U64,
    reclaim_after: U64,
    fee_bps_max: u16,
    terms_hash: String,
    salt: String,
}

/// El `msg` `authorize` ya validado: `v == 1`, `action == "authorize"` y los hex decodificados.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizeMsg {
    pub operator_id: String,
    pub receiver: AccountId,
    pub release_deadline: u64,
    pub reclaim_after: u64,
    pub fee_bps_max: u16,
    pub terms_hash: [u8; 32],
    pub salt: [u8; 16],
}

impl AuthorizeMsg {
    /// El JSON canonico de la sec. 3.2: estas claves, este orden, sin espacios, hex en minusculas.
    pub fn to_canonical_json(&self) -> String {
        let json = AuthorizeMsgJson {
            v: MSG_VERSION,
            action: "authorize".into(),
            operator_id: self.operator_id.clone(),
            receiver: self.receiver.clone(),
            release_deadline: U64(self.release_deadline),
            reclaim_after: U64(self.reclaim_after),
            fee_bps_max: self.fee_bps_max,
            terms_hash: hex_encode(&self.terms_hash),
            salt: hex_encode(&self.salt),
        };
        serde_json::to_string(&json).unwrap_or_else(|_| env::abort())
    }

    /// `terms_hash` como se guarda en el `Payment`: hex de 64 en minusculas.
    pub fn terms_hash_hex(&self) -> String {
        hex_encode(&self.terms_hash)
    }
}

/// Parsea el `msg` de `ft_transfer_call`. Cualquier falla (JSON, campo desconocido o repetido, `v`, `action`, hex)
/// es `E03_MSG_INVALIDO`. El hex se exige en minusculas y con el largo exacto (64 y 32), sin `0x`.
pub fn parse_authorize_msg(msg: &str) -> Result<AuthorizeMsg, ArrasError> {
    let json: AuthorizeMsgJson =
        serde_json::from_str(msg).map_err(|_| ArrasError::E03MsgInvalido)?;
    if json.v != MSG_VERSION || json.action != "authorize" {
        return Err(ArrasError::E03MsgInvalido);
    }
    let terms_hash = hex_decode::<32>(&json.terms_hash).ok_or(ArrasError::E03MsgInvalido)?;
    let salt = hex_decode::<16>(&json.salt).ok_or(ArrasError::E03MsgInvalido)?;
    Ok(AuthorizeMsg {
        operator_id: json.operator_id,
        receiver: json.receiver,
        release_deadline: json.release_deadline.0,
        reclaim_after: json.reclaim_after.0,
        fee_bps_max: json.fee_bps_max,
        terms_hash,
        salt,
    })
}

/// Lo que entra en la preimagen del `payment_id`.
#[derive(Debug, Clone, Copy)]
pub struct PaymentIdInput<'a> {
    pub operator_id: &'a str,
    pub token: &'a str,
    pub payer: &'a str,
    pub receiver: &'a str,
    pub amount: u128,
    pub release_deadline: u64,
    pub reclaim_after: u64,
    pub fee_bps_max: u16,
    pub terms_hash: &'a [u8; 32],
    pub salt: &'a [u8; 16],
}

impl<'a> PaymentIdInput<'a> {
    /// La entrada del `payment_id` desde un `msg` ya parseado: `payer` es el `sender_id` de `ft_on_transfer`
    /// (nunca el predecessor, que es el token).
    pub fn from_msg(msg: &'a AuthorizeMsg, token: &'a str, payer: &'a str, amount: u128) -> Self {
        Self {
            operator_id: &msg.operator_id,
            token,
            payer,
            receiver: msg.receiver.as_str(),
            amount,
            release_deadline: msg.release_deadline,
            reclaim_after: msg.reclaim_after,
            fee_bps_max: msg.fee_bps_max,
            terms_hash: &msg.terms_hash,
            salt: &msg.salt,
        }
    }
}

#[derive(BorshSerialize)]
#[borsh(crate = "near_sdk::borsh")]
struct Preimage<'a> {
    v: u8,
    operator_id: &'a str,
    token: &'a str,
    payer: &'a str,
    receiver: &'a str,
    amount: u128,
    release_deadline: u64,
    reclaim_after: u64,
    fee_bps_max: u16,
    terms_hash: [u8; 32],
    salt: [u8; 16],
}

/// `borsh(preimagen)` (202 bytes para V-1).
pub fn payment_id_preimage(input: &PaymentIdInput<'_>) -> Vec<u8> {
    let p = Preimage {
        v: MSG_VERSION,
        operator_id: input.operator_id,
        token: input.token,
        payer: input.payer,
        receiver: input.receiver,
        amount: input.amount,
        release_deadline: input.release_deadline,
        reclaim_after: input.reclaim_after,
        fee_bps_max: input.fee_bps_max,
        terms_hash: *input.terms_hash,
        salt: *input.salt,
    };
    borsh::to_vec(&p).unwrap_or_else(|_| env::abort())
}

/// `(sha256(borsh(preimagen)), base58 de ese hash)`.
pub fn compute_payment_id(input: &PaymentIdInput<'_>) -> ([u8; 32], PaymentId) {
    let hash = env::sha256_array(payment_id_preimage(input));
    (hash, bs58::encode(hash).into_string())
}

/// El hash de 32 bytes de un `payment_id` en su forma canonica (la que sale de [`compute_payment_id`]): cualquier
/// largo de 32 a 44 que vuelva igual al re-codificar.
pub fn decode_payment_id(id: &str) -> Option<[u8; 32]> {
    if !(32..=44).contains(&id.len()) {
        return None;
    }
    let mut out = [0u8; 32];
    let n = bs58::decode(id).onto(&mut out).ok()?;
    if n != 32 || bs58::encode(out).into_string() != id {
        return None;
    }
    Some(out)
}

const HEX: &[u8; 16] = b"0123456789abcdef";

fn hex_encode(bytes: &[u8]) -> String {
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

/// Hex en minusculas, exactamente `2 * N` caracteres.
fn hex_decode<const N: usize>(s: &str) -> Option<[u8; N]> {
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

#[cfg(test)]
mod tests {
    use super::*;

    const USDC_TESTNET: &str = "3e2210e1184b45b64c8a434c0a7e7b23cc04ea7eb7a6c3c32520d03d4afcb8af";
    const USDC_MAINNET: &str = "17208628f84f5d6ad33f0da3bbbeb27ffcb398eac501a31bd6ad2011e36133a1";
    const RELEASE: u64 = 1_793_577_600_000_000_000; // 2026-11-02T00:00Z
    const RECLAIM: u64 = 1_793_664_000_000_000_000; // 2026-11-03T00:00Z
    const V1_PREIMAGE_HEX: &str = "0102000000656d40000000336532323130653131383462343562363463386134333463306137653762323363633034656137656237613663336333323532306430336434616663623861661200000070617965722d64656d6f2e746573746e657413000000776f726b65722d64656d6f2e746573746e657440420f000000000000000000000000000000190d5311e4180000689ee75fe4181405000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";

    fn h32(s: &str) -> [u8; 32] {
        hex_decode::<32>(s).unwrap()
    }

    struct Vector {
        nombre: &'static str,
        token: &'static str,
        payer: &'static str,
        receiver: &'static str,
        amount: u128,
        fee_bps_max: u16,
        terms_hash: [u8; 32],
        salt: [u8; 16],
        bytes: usize,
        sha256: &'static str,
        payment_id: &'static str,
    }

    fn vectores() -> [Vector; 3] {
        let mut salt_v2 = [0u8; 16];
        salt_v2[15] = 1;
        [
            Vector {
                nombre: "V-1",
                token: USDC_TESTNET,
                payer: "payer-demo.testnet",
                receiver: "worker-demo.testnet",
                amount: 1_000_000,
                fee_bps_max: 1300,
                terms_hash: [0; 32],
                salt: [0; 16],
                bytes: 202,
                sha256: "b7b2b0e5d5108ea9a57677a8c6a8db75f5a3a831dd79c8aa64fbaf5b1c8573b2",
                payment_id: "DN5eMA3sNES7xsoy6qvy24cVF6Dmwjxpkvd2b4e8P38M",
            },
            Vector {
                nombre: "V-2",
                token: USDC_TESTNET,
                payer: "payer-demo.testnet",
                receiver: "worker-demo.testnet",
                amount: 1_000_000,
                fee_bps_max: 1300,
                terms_hash: [0; 32],
                salt: salt_v2,
                bytes: 202,
                sha256: "068f284abb07df3b822f07b5ea6463ccfbcbc79cb505350482aa684b4df90be8",
                payment_id: "Sc4D4Jn3ErWE68FckdDTLzThk1FTR1ZfLzuBUE4EeRm",
            },
            Vector {
                nombre: "V-3",
                token: USDC_MAINNET,
                payer: "payer-demo.near",
                receiver: "robot-k1.near",
                amount: 10_000_000,
                fee_bps_max: 1800,
                terms_hash: h32("134931ea6d298d41174d2200087b1c4a38c81987f1af576b4601921af65808d8"),
                salt: [0xab; 16],
                bytes: 193,
                sha256: "1bda3698bc2574284b8a54b4078007aff35bf04514e8c66eae4e5c96f49ca359",
                payment_id: "2sizp6qpQdKz3DPmd1kX2ovobag9LfXerQKmEPQV3mb6",
            },
        ]
    }

    /// U-01 `payment_id_vectores`: V-1..V-3 exactos, byte a byte (preimagen de V-1 completa, sha256 y base58).
    #[test]
    fn payment_id_vectores() {
        for v in vectores() {
            let input = PaymentIdInput {
                operator_id: "em",
                token: v.token,
                payer: v.payer,
                receiver: v.receiver,
                amount: v.amount,
                release_deadline: RELEASE,
                reclaim_after: RECLAIM,
                fee_bps_max: v.fee_bps_max,
                terms_hash: &v.terms_hash,
                salt: &v.salt,
            };
            let pre = payment_id_preimage(&input);
            assert_eq!(pre.len(), v.bytes, "{} bytes", v.nombre);
            if v.nombre == "V-1" {
                assert_eq!(hex_encode(&pre), V1_PREIMAGE_HEX, "preimagen V-1");
            }
            let (hash, id) = compute_payment_id(&input);
            assert_eq!(hex_encode(&hash), v.sha256, "{} sha256", v.nombre);
            assert_eq!(id, v.payment_id, "{} payment_id", v.nombre);
            assert_eq!(decode_payment_id(&id), Some(hash), "{} decode", v.nombre);
        }
        // V-2 tiene 43 caracteres: nadie puede asumir 44.
        assert_eq!(vectores()[1].payment_id.len(), 43);
        // terms_hash de V-3 = sha256("em-task-0001").
        assert_eq!(env::sha256_array(b"em-task-0001"), vectores()[2].terms_hash);
    }

    fn msg_v1() -> String {
        format!(
            "{{\"v\":1,\"action\":\"authorize\",\"operator_id\":\"em\",\"receiver\":\"worker-demo.testnet\",\"release_deadline\":\"{RELEASE}\",\"reclaim_after\":\"{RECLAIM}\",\"fee_bps_max\":1300,\"terms_hash\":\"{}\",\"salt\":\"{}\"}}",
            "0".repeat(64),
            "0".repeat(32)
        )
    }

    #[test]
    fn msg_canonico_v1_ida_y_vuelta_y_payment_id() {
        let msg = parse_authorize_msg(&msg_v1()).unwrap();
        assert_eq!(msg.to_canonical_json(), msg_v1());
        let input = PaymentIdInput::from_msg(&msg, USDC_TESTNET, "payer-demo.testnet", 1_000_000);
        assert_eq!(
            compute_payment_id(&input).1,
            "DN5eMA3sNES7xsoy6qvy24cVF6Dmwjxpkvd2b4e8P38M"
        );
    }

    #[test]
    fn msg_invalido_es_e03() {
        let base = msg_v1();
        let malos = [
            String::new(),
            "no-json".into(),
            "[]".into(),
            base.replace("\"v\":1", "\"v\":2"),
            base.replace("\"v\":1", "\"v\":\"1\""),
            base.replace("\"action\":\"authorize\"", "\"action\":\"budget\""),
            base.replace("\"action\":\"authorize\"", "\"action\":\"Authorize\""),
            base.replace("}", ",\"extra\":1}"),
            base.replace("\"v\":1,", "\"v\":1,\"v\":1,"),
            base.replace("\"v\":1,", ""),
            base.replace(&"0".repeat(64), &"0".repeat(63)),
            base.replace(&"0".repeat(64), &format!("0x{}", "0".repeat(62))),
            base.replace(&"0".repeat(64), &"A".repeat(64)),
            base.replace(&"0".repeat(64), &"g".repeat(64)),
            base.replace(&"0".repeat(32), &"0".repeat(34)),
            base.replace("worker-demo.testnet", "Worker-Demo.testnet"),
            base.replace("worker-demo.testnet", ""),
            base.replace(&format!("\"{RELEASE}\""), &RELEASE.to_string()),
            base.replace("\"fee_bps_max\":1300", "\"fee_bps_max\":70000"),
            base.replace("\"fee_bps_max\":1300", "\"fee_bps_max\":-1"),
            format!("{base}x"),
        ];
        for m in malos {
            assert_eq!(
                parse_authorize_msg(&m),
                Err(ArrasError::E03MsgInvalido),
                "{m}"
            );
        }
        // El contrato no exige el orden canonico: el id sale de los valores, no de los bytes del JSON
        // (el SDK y el facilitador comparan el canonico byte a byte, sec. 3.2).
        assert!(parse_authorize_msg(&base.replace("{\"v\":1,", "{ \"v\" : 1 ,")).is_ok());
    }

    #[test]
    fn operator_id_valido() {
        for ok in ["em", "kk", "a", "a-b_c9", &"x".repeat(32)] {
            assert!(is_valid_operator_id(ok), "{ok}");
        }
        for malo in ["", "EM", "e m", "em.", "ém", &"x".repeat(33)] {
            assert!(!is_valid_operator_id(malo), "{malo}");
        }
    }

    /// P2-1 de la refutacion: un hash con bytes altos chicos da un id de 42 caracteres (y 32 si es todo ceros);
    /// tiene que decodificar igual.
    #[test]
    fn payment_id_de_menos_de_43_caracteres() {
        let mut m = parse_authorize_msg(&msg_v1()).unwrap();
        m.salt[14] = 0x04;
        m.salt[15] = 0x15;
        let (hash, id) = compute_payment_id(&PaymentIdInput::from_msg(
            &m,
            USDC_TESTNET,
            "payer-demo.testnet",
            1_000_000,
        ));
        assert_eq!(id, "1y1ZbrieUbaioiUBHxX7EyZMNkCis2tLKCs5Xxezgy");
        assert_eq!(id.len(), 42);
        assert_eq!(decode_payment_id(&id), Some(hash));
        let ceros = bs58::encode([0u8; 32]).into_string();
        assert_eq!(ceros.len(), 32);
        assert_eq!(decode_payment_id(&ceros), Some([0u8; 32]));
        assert_eq!(decode_payment_id(&"1".repeat(31)), None);
    }

    #[test]
    fn decode_payment_id_rechaza_formas_no_canonicas() {
        assert!(decode_payment_id("DN5eMA3sNES7xsoy6qvy24cVF6Dmwjxpkvd2b4e8P38M").is_some());
        // 45 y 42 caracteres, un '1' de mas adelante (otro largo de bytes), una 'l' fuera del alfabeto y 32 bytes con
        // ceros adelante escritos sin el '1' que les toca.
        for malo in [
            "",
            "0",
            "DN5eMA3sNES7xsoy6qvy24cVF6Dmwjxpkvd2b4e8P38MM",
            "DN5eMA3sNES7xsoy6qvy24cVF6Dmwjxpkvd2b4e8P3",
            "1DN5eMA3sNES7xsoy6qvy24cVF6Dmwjxpkvd2b4e8P38M",
            "DN5eMA3sNES7xsoy6qvy24cVF6Dmwjxpkvd2b4e8P38l",
            "11111111111111111111111111111111111111111112",
        ] {
            assert_eq!(decode_payment_id(malo), None, "{malo}");
        }
    }
}
