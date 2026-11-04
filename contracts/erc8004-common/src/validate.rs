//! Validacion de entradas. Todo es por bytes: nada se normaliza (ni mayusculas ni espacios); lo que no cumple, se
//! rechaza.
//!
//! - URI: de 1 al tope de bytes, solo los caracteres de RFC 3986 (unreserved, reserved y `%`: ni espacios ni `"` ni
//!   `\`, que JSON escaparia al doble en los logs), con un esquema adelante (`ALPHA *( ALPHA / DIGIT / "+" / "-" /
//!   "." ) ":"`) y algo despues de los dos puntos. Sirven `https://`, `ipfs://`, `ar://`, `data:` y `did:`. El
//!   percent-encoding no se decodifica ni se valida.
//! - clave de metadata: de 1 a 64 bytes de `[A-Za-z0-9._:/-]` (distingue mayusculas, como el `keccak256` de EVM).
//! - tag: de 0 a 64 bytes de UTF-8 sin caracteres de control (Unicode `Cc`); `""` es "sin tag", como en EVM.
//! - hash: exactamente 64 hex en minusculas, sin `0x` (como el `terms_hash` de Arras y registry-v1).
//! - enteros sin signo: decimales canonicos (solo digitos, sin ceros a la izquierda salvo `"0"`).
//! - `value` (`int128` de EVM): un `-` opcional y un decimal canonico; `"-0"` no vale.
//! - `role` y `context` de una prueba: de 0 a 32 bytes de `[a-z0-9_-]`.
//! - `proof`: de 1 a 128 bytes de ASCII imprimible sin espacios.

/// `10^38`, el `MAX_ABS_VALUE` de `ReputationRegistryUpgradeable`.
pub const MAX_ABS_VALUE: u128 = 100_000_000_000_000_000_000_000_000_000_000_000_000;
/// `valueDecimals` maximo (EVM: `"too many decimals"`).
pub const MAX_VALUE_DECIMALS: u8 = 18;
pub const MAX_METADATA_KEY_LEN: usize = 64;
pub const MAX_TAG_LEN: usize = 64;
pub const MAX_LABEL_LEN: usize = 32;
pub const MAX_PROOF_LEN: usize = 128;

/// Un byte de URI de RFC 3986: `unreserved` (`ALPHA DIGIT - . _ ~`), `reserved` (`: / ? # [ ] @ ! $ & ' ( ) * + , ;
/// =`) o `%`.
pub fn is_uri_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"-._~:/?#[]@!$&'()*+,;=%".contains(&b)
}

/// URI con esquema, de 1 a `max` bytes de RFC 3986.
pub fn is_valid_uri(s: &str, max: usize) -> bool {
    let b = s.as_bytes();
    if b.is_empty() || b.len() > max || !b.iter().all(|&c| is_uri_byte(c)) {
        return false;
    }
    let Some(colon) = b.iter().position(|&c| c == b':') else {
        return false;
    };
    // `b[0]` letra implica `colon >= 1`: va antes del slice.
    colon + 1 < b.len()
        && b[0].is_ascii_alphabetic()
        && b[1..colon]
            .iter()
            .all(|&c| c.is_ascii_alphanumeric() || c == b'+' || c == b'-' || c == b'.')
}

/// `""` o una URI valida de hasta `max` bytes.
pub fn is_valid_optional_uri(s: &str, max: usize) -> bool {
    s.is_empty() || is_valid_uri(s, max)
}

/// Clave de metadata: `[A-Za-z0-9._:/-]{1,64}`.
pub fn is_valid_metadata_key(k: &str) -> bool {
    !k.is_empty()
        && k.len() <= MAX_METADATA_KEY_LEN
        && k.bytes().all(|c| {
            c.is_ascii_alphanumeric()
                || c == b'.'
                || c == b'_'
                || c == b':'
                || c == b'/'
                || c == b'-'
        })
}

/// Tag: de 0 a 64 bytes, sin caracteres de control.
pub fn is_valid_tag(t: &str) -> bool {
    t.len() <= MAX_TAG_LEN && !t.chars().any(char::is_control)
}

/// `role` / `context` de una prueba: `[a-z0-9_-]{0,32}`.
pub fn is_valid_label(s: &str) -> bool {
    s.len() <= MAX_LABEL_LEN
        && s.bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_' || c == b'-')
}

/// `proof`: de 1 a 128 bytes de ASCII imprimible sin espacios.
pub fn is_valid_proof(p: &str) -> bool {
    !p.is_empty() && p.len() <= MAX_PROOF_LEN && p.bytes().all(|c| (0x21..=0x7e).contains(&c))
}

/// Un entero sin signo como lo escribe `U128`/`U64` de near-sdk: solo digitos, sin ceros a la izquierda (salvo
/// `"0"`). El rango lo mira quien lo parsea.
pub fn is_canonical_uint(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty() && b.iter().all(u8::is_ascii_digit) && (b[0] != b'0' || b.len() == 1)
}

/// Un `u64` canonico (`agent_id`, `feedback_index`, `deadline`): `None` si no es canonico o no entra.
pub fn parse_u64(s: &str) -> Option<u64> {
    if is_canonical_uint(s) {
        s.parse().ok()
    } else {
        None
    }
}

/// Un `u128` canonico.
pub fn parse_u128(s: &str) -> Option<u128> {
    if is_canonical_uint(s) {
        s.parse().ok()
    } else {
        None
    }
}

/// El `value` de una calificacion: `-?` + decimal canonico, sin `"-0"`, dentro de `i128`. El tope de `10^38` lo mira
/// quien llama (es otro error, como en EVM).
pub fn parse_value(s: &str) -> Option<i128> {
    let (neg, digits) = match s.strip_prefix('-') {
        Some(d) => (true, d),
        None => (false, s),
    };
    if !is_canonical_uint(digits) || (neg && digits == "0") {
        return None;
    }
    s.parse().ok()
}

/// 64 hex en minusculas -> 32 bytes.
pub fn parse_hash(h: &str) -> Option<[u8; 32]> {
    let b = h.as_bytes();
    if b.len() != 64 {
        return None;
    }
    let nibble = |c: u8| match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        _ => None,
    };
    let mut out = [0u8; 32];
    for (i, par) in b.chunks_exact(2).enumerate() {
        out[i] = (nibble(par[0])? << 4) | nibble(par[1])?;
    }
    Some(out)
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
    fn uris() {
        for ok in [
            "https://a.example/x.json",
            "ipfs://bafy",
            "ar://x",
            "data:application/json;base64,eyJ0eXBlIjoi",
            "did:near:agente.near",
            "HTTPS://A",
            "a:b",
            "x+y-z.w:1",
            "https://a.example/%20?q=1&r=[2]#f~'()*,;=!$@",
        ] {
            assert!(is_valid_uri(ok, 100), "{ok}");
        }
        for bad in [
            "",
            "sin-esquema",
            ":x",
            "1a:x",
            "a:",
            "https://a b",
            " https://a",
            "https://a\"b",
            "https://a\\b",
            "https://a<b>",
            "https://a{b}",
            "https://a|b",
            "https://a^b",
            "https://a`b",
            "https://\u{e9}",
            "h_t:x",
            "h/t:x",
        ] {
            assert!(!is_valid_uri(bad, 100), "{bad:?}");
        }
        assert!(is_valid_uri(&format!("a:{}", "b".repeat(8)), 10));
        assert!(!is_valid_uri(&format!("a:{}", "b".repeat(9)), 10));
        assert!(is_valid_optional_uri("", 10));
        assert!(!is_valid_optional_uri(" ", 10));
    }

    #[test]
    fn claves_tags_labels_y_proof() {
        assert!(is_valid_metadata_key("agentName"));
        assert!(is_valid_metadata_key("a.b_c:d/e-F9"));
        assert!(is_valid_metadata_key(&"k".repeat(64)));
        for bad in ["", " k", "k ", "k\u{0}", "\u{f1}", "k=v", &"k".repeat(65)] {
            assert!(!is_valid_metadata_key(bad), "{bad:?}");
        }
        assert!(is_valid_tag(""));
        assert!(is_valid_tag("successRate"));
        assert!(is_valid_tag("trading yield \u{f1}and\u{fa}"));
        assert!(is_valid_tag(&"t".repeat(64)));
        assert!(!is_valid_tag(&"t".repeat(65)));
        // 32 caracteres de 2 bytes = 64 bytes: vale; uno mas no.
        assert!(is_valid_tag(&"\u{f1}".repeat(32)));
        assert!(!is_valid_tag(&"\u{f1}".repeat(33)));
        for bad in ["a\u{0}", "a\n", "a\u{7f}", "a\u{85}", "\u{9f}"] {
            assert!(!is_valid_tag(bad), "{bad:?}");
        }
        assert!(is_valid_label(""));
        assert!(is_valid_label("em"));
        assert!(is_valid_label("receiver"));
        assert!(is_valid_label(&"a".repeat(32)));
        for bad in ["EM", "e m", "e.m", &"a".repeat(33)] {
            assert!(!is_valid_label(bad), "{bad:?}");
        }
        assert!(is_valid_proof(
            "DN5eMA3sNES7xsoy6qvy24cVF6Dmwjxpkvd2b4e8P38M"
        ));
        assert!(is_valid_proof(&"p".repeat(128)));
        for bad in ["", " p", "p q", "p\u{7f}", "\u{f1}", &"p".repeat(129)] {
            assert!(!is_valid_proof(bad), "{bad:?}");
        }
    }

    #[test]
    fn enteros_canonicos() {
        assert_eq!(parse_u64("0"), Some(0));
        assert_eq!(parse_u64("17"), Some(17));
        assert_eq!(parse_u64("18446744073709551615"), Some(u64::MAX));
        for bad in [
            "",
            "01",
            "00",
            "+1",
            "-1",
            " 1",
            "1 ",
            "1e3",
            "0x1",
            "1_0",
            "18446744073709551616",
        ] {
            assert_eq!(parse_u64(bad), None, "{bad:?}");
        }
        assert_eq!(
            parse_u128("340282366920938463463374607431768211455"),
            Some(u128::MAX)
        );
        assert_eq!(parse_u128("340282366920938463463374607431768211456"), None);
        assert_eq!(parse_u128("007"), None);
    }

    #[test]
    fn values() {
        assert_eq!(parse_value("0"), Some(0));
        assert_eq!(parse_value("87"), Some(87));
        assert_eq!(parse_value("-32"), Some(-32));
        assert_eq!(
            parse_value("100000000000000000000000000000000000000"),
            Some(MAX_ABS_VALUE as i128)
        );
        assert_eq!(parse_value(&i128::MIN.to_string()), Some(i128::MIN));
        assert_eq!(parse_value(&i128::MAX.to_string()), Some(i128::MAX));
        for bad in [
            "",
            "-",
            "-0",
            "+1",
            "01",
            "-01",
            "--1",
            " 1",
            "1.0",
            "1e2",
            "0x1",
            "170141183460469231731687303715884105728",
            "-170141183460469231731687303715884105729",
        ] {
            assert_eq!(parse_value(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn hashes() {
        let h = "00ff10a0b1c2d3e4f5061728394a5b6c7d8e9fa0b1c2d3e4f5061728394a5b6c";
        let b = parse_hash(h).unwrap();
        assert_eq!(b[0], 0x00);
        assert_eq!(b[1], 0xff);
        assert_eq!(hex(&b), h);
        assert!(parse_hash(&h.to_uppercase()).is_none());
        assert!(parse_hash(&format!("0x{}", &h[2..])).is_none());
        assert!(parse_hash(&h[1..]).is_none());
        assert!(parse_hash(&format!("{h}0")).is_none());
        assert!(parse_hash(&format!("g{}", &h[1..])).is_none());
        assert!(parse_hash("").is_none());
    }
}
