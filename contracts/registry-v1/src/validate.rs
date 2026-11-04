//! Validacion de las entradas. Todo es por bytes: nada se normaliza (ni mayusculas ni espacios); lo que no cumple, se
//! rechaza.
//!
//! - URI (`agent_uri`, `feedback_uri`): de 1 al tope de bytes, ASCII imprimible sin espacios (`0x21..=0x7e`: lo demas
//!   va con percent-encoding) y un esquema de RFC 3986 adelante (`ALPHA *( ALPHA / DIGIT / "+" / "-" / "." ) ":"`)
//!   con algo despues de los dos puntos. Sirven `https://`, `ipfs://`, `ar://`, `data:` y `did:`.
//! - `metadata`: de 1 a 1024 bytes de UTF-8 sin caracteres de control ASCII (`0x00..=0x1f` y `0x7f`).
//! - tag: de 1 a 64 bytes de `[A-Za-z0-9._:/-]`; distingue mayusculas.
//! - `payment_id`: de 1 a 64 bytes de ASCII imprimible sin espacios.
//! - `feedback_hash`: exactamente 64 hex en minusculas, sin `0x` (como el `terms_hash` de Arras).
//! - Los `U128`/`U64` de la respuesta de `get_payment`: decimales canonicos (`is_canonical_uint`).

use crate::{
    MAX_AGENT_URI_LEN, MAX_FEEDBACK_URI_LEN, MAX_METADATA_LEN, MAX_PAYMENT_ID_LEN, MAX_TAG_LEN,
    RegistryError,
};

fn printable_no_space(b: u8) -> bool {
    (0x21..=0x7e).contains(&b)
}

/// URI con esquema, ASCII imprimible sin espacios, de 1 a `max` bytes.
pub fn is_valid_uri(s: &str, max: usize) -> bool {
    let b = s.as_bytes();
    if b.is_empty() || b.len() > max || !b.iter().all(|&c| printable_no_space(c)) {
        return false;
    }
    let Some(colon) = b.iter().position(|&c| c == b':') else {
        return false;
    };
    // `b[0]` letra implica `colon >= 1` (y que `b[1..colon]` sea un rango valido): va antes del slice.
    colon + 1 < b.len()
        && b[0].is_ascii_alphabetic()
        && b[1..colon]
            .iter()
            .all(|&c| c.is_ascii_alphanumeric() || c == b'+' || c == b'-' || c == b'.')
}

pub fn check_agent_uri(s: &str) -> Result<(), RegistryError> {
    if is_valid_uri(s, MAX_AGENT_URI_LEN) {
        Ok(())
    } else {
        Err(RegistryError::E01AgentUriInvalida)
    }
}

pub fn check_metadata(m: Option<&str>) -> Result<(), RegistryError> {
    let Some(m) = m else { return Ok(()) };
    let ok =
        !m.is_empty() && m.len() <= MAX_METADATA_LEN && !m.bytes().any(|c| c < 0x20 || c == 0x7f);
    if ok {
        Ok(())
    } else {
        Err(RegistryError::E02MetadataInvalida)
    }
}

pub fn is_valid_tag(t: &str) -> bool {
    !t.is_empty()
        && t.len() <= MAX_TAG_LEN
        && t.bytes().all(|c| {
            c.is_ascii_alphanumeric()
                || c == b'.'
                || c == b'_'
                || c == b':'
                || c == b'/'
                || c == b'-'
        })
}

pub fn check_tag(t: Option<&str>) -> Result<(), RegistryError> {
    match t {
        Some(t) if !is_valid_tag(t) => Err(RegistryError::E13TagInvalido),
        _ => Ok(()),
    }
}

pub fn check_feedback_uri(u: Option<&str>) -> Result<(), RegistryError> {
    match u {
        Some(u) if !is_valid_uri(u, MAX_FEEDBACK_URI_LEN) => {
            Err(RegistryError::E14FeedbackUriInvalida)
        }
        _ => Ok(()),
    }
}

/// Un entero sin signo como lo escribe `U128`/`U64` de near-sdk: solo digitos, sin ceros a la izquierda (salvo `"0"`).
/// El rango lo mira quien lo parsea.
pub fn is_canonical_uint(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty() && b.iter().all(u8::is_ascii_digit) && (b[0] != b'0' || b.len() == 1)
}

pub fn check_payment_id(id: &str) -> Result<(), RegistryError> {
    let b = id.as_bytes();
    if !b.is_empty() && b.len() <= MAX_PAYMENT_ID_LEN && b.iter().all(|&c| printable_no_space(c)) {
        Ok(())
    } else {
        Err(RegistryError::E11PaymentIdInvalido)
    }
}

/// 64 hex en minusculas -> 32 bytes.
pub fn parse_hash(h: &str) -> Result<[u8; 32], RegistryError> {
    let b = h.as_bytes();
    if b.len() != 64 {
        return Err(RegistryError::E15FeedbackHashInvalido);
    }
    let nibble = |c: u8| match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        _ => None,
    };
    let mut out = [0u8; 32];
    for (i, par) in b.chunks_exact(2).enumerate() {
        match (nibble(par[0]), nibble(par[1])) {
            (Some(hi), Some(lo)) => out[i] = (hi << 4) | lo,
            _ => return Err(RegistryError::E15FeedbackHashInvalido),
        }
    }
    Ok(out)
}

/// 32 bytes -> 64 hex en minusculas.
pub fn hex(bytes: &[u8; 32]) -> String {
    const D: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(64);
    for &b in bytes {
        s.push(D[(b >> 4) as usize] as char);
        s.push(D[(b & 0x0f) as usize] as char);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_validas() {
        for u in [
            "https://agente.example/card.json",
            "HTTPS://AGENTE.EXAMPLE",
            "ipfs://bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi",
            "ar://abc",
            "data:application/json;base64,eyJ9",
            "did:near:agente.near",
            "a:b",
            "x+y-z.w:1",
        ] {
            assert!(is_valid_uri(u, 512), "{u}");
        }
        let tope = format!("https://{}", "a".repeat(512 - 8));
        assert_eq!(tope.len(), 512);
        assert!(is_valid_uri(&tope, 512));
    }

    #[test]
    fn uri_invalidas() {
        let larga = format!("https://{}", "a".repeat(512 - 7));
        for u in [
            "",
            " https://x",
            "https://x ",
            "https://a b",
            "https://x\t",
            "https://x\n",
            "https://\u{7f}",
            "https://agénte",
            "sin-esquema",
            ":solo-dos-puntos",
            "1http://x",
            "ht_tp://x",
            "https:",
            "a:",
            larga.as_str(),
        ] {
            assert!(!is_valid_uri(u, 512), "{u:?}");
        }
    }

    #[test]
    fn metadata() {
        assert_eq!(check_metadata(None), Ok(()));
        assert_eq!(check_metadata(Some(r#"{"nombre":"Agente ñandú"}"#)), Ok(()));
        assert_eq!(check_metadata(Some(&"m".repeat(1024))), Ok(()));
        for m in ["", "a\nb", "a\tb", "\u{0}", "\u{7f}"] {
            assert_eq!(
                check_metadata(Some(m)),
                Err(RegistryError::E02MetadataInvalida),
                "{m:?}"
            );
        }
        assert_eq!(
            check_metadata(Some(&"m".repeat(1025))),
            Err(RegistryError::E02MetadataInvalida)
        );
    }

    #[test]
    fn tags() {
        for t in [
            "quality",
            "successRate",
            "em:task/123",
            "a.b_c-d",
            &"t".repeat(64),
        ] {
            assert_eq!(check_tag(Some(t)), Ok(()), "{t}");
        }
        assert_eq!(check_tag(None), Ok(()));
        for t in [
            "",
            " quality",
            "quality ",
            "cali dad",
            "ñ",
            "a#b",
            &"t".repeat(65),
        ] {
            assert_eq!(
                check_tag(Some(t)),
                Err(RegistryError::E13TagInvalido),
                "{t:?}"
            );
        }
    }

    #[test]
    fn feedback_uri() {
        assert_eq!(check_feedback_uri(None), Ok(()));
        assert_eq!(check_feedback_uri(Some("https://x/y")), Ok(()));
        assert_eq!(
            check_feedback_uri(Some("")),
            Err(RegistryError::E14FeedbackUriInvalida)
        );
        let larga = format!("https://{}", "a".repeat(512 - 7));
        assert_eq!(
            check_feedback_uri(Some(&larga)),
            Err(RegistryError::E14FeedbackUriInvalida)
        );
    }

    #[test]
    fn payment_id() {
        assert_eq!(
            check_payment_id("DN5eMA3sNES7xsoy6qvy24cVF6Dmwjxpkvd2b4e8P38M"),
            Ok(())
        );
        assert_eq!(check_payment_id(&"p".repeat(64)), Ok(()));
        for id in ["", " x", "x y", "x\n", "pagó", &"p".repeat(65)] {
            assert_eq!(
                check_payment_id(id),
                Err(RegistryError::E11PaymentIdInvalido),
                "{id:?}"
            );
        }
    }

    #[test]
    fn enteros_canonicos() {
        for ok in ["0", "1", "10", "1000000", &u128::MAX.to_string()] {
            assert!(is_canonical_uint(ok), "{ok}");
        }
        for mal in [
            "", "+1", "-1", "01", "00", "000", " 1", "1 ", "1_000", "0x1", "1e0", "1.0", "١",
        ] {
            assert!(!is_canonical_uint(mal), "{mal:?}");
        }
    }

    #[test]
    fn hash_hex() {
        let h = "00ff10a0b1c2d3e4f5061728394a5b6c7d8e9fa0b1c2d3e4f5061728394a5b6c";
        let b = parse_hash(h).unwrap();
        assert_eq!(b[0], 0x00);
        assert_eq!(b[1], 0xff);
        assert_eq!(b[2], 0x10);
        assert_eq!(hex(&b), h);
        let todo_f = "f".repeat(64);
        assert_eq!(parse_hash(&todo_f).unwrap(), [0xff; 32]);
        for mal in [
            "",
            &"0".repeat(63),
            &"0".repeat(65),
            &format!("0x{}", "0".repeat(62)),
            &format!("0X{}", "0".repeat(62)),
            &"F".repeat(64),
            &format!("{}g", "0".repeat(63)),
            &format!("{} ", "0".repeat(63)),
            &format!("{}/", "0".repeat(63)),
            &format!("{}:", "0".repeat(63)),
            &format!("{}`", "0".repeat(63)),
        ] {
            assert_eq!(
                parse_hash(mal),
                Err(RegistryError::E15FeedbackHashInvalido),
                "{mal:?}"
            );
        }
    }
}
