//! Eventos NEP-297: `EVENT_JSON:{"standard":...,"version":...,"event":...,"data":[{...}]}`, con `data` siempre un
//! arreglo de un elemento. Los registros usan `standard` [`STANDARD`] y `version` [`VERSION`]; el NFT de Identity,
//! ademas, los de NEP-171 ([`NEP171_STANDARD`] 1.0.0: `nft_mint`, `nft_transfer`).

use near_sdk::serde::Serialize;
use near_sdk::{env, serde_json};

/// `standard` de los eventos de los registros ERC-8004 en NEAR.
pub const STANDARD: &str = "erc8004";
/// `version` del esquema de los eventos (la del esquema NEAR, no la de la EIP).
pub const VERSION: &str = "1.0.0";
/// NEP-171 (core de NFT).
pub const NEP171_STANDARD: &str = "nep171";
pub const NEP171_VERSION: &str = "1.0.0";

#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
struct Envelope<'a, T: Serialize> {
    standard: &'a str,
    version: &'a str,
    event: &'a str,
    data: [&'a T; 1],
}

/// El log completo, sin emitirlo (para los tests y para los SDK).
pub fn event_log<T: Serialize>(standard: &str, version: &str, event: &str, data: &T) -> String {
    let e = Envelope {
        standard,
        version,
        event,
        data: [data],
    };
    format!(
        "EVENT_JSON:{}",
        serde_json::to_string(&e).unwrap_or_else(|_| env::abort())
    )
}

/// Emite un evento `erc8004` 1.0.0.
pub fn emit<T: Serialize>(event: &str, data: &T) {
    env::log_str(&event_log(STANDARD, VERSION, event, data));
}

/// Emite un evento `nep171` 1.0.0.
pub fn emit_nep171<T: Serialize>(event: &str, data: &T) {
    env::log_str(&event_log(NEP171_STANDARD, NEP171_VERSION, event, data));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize)]
    #[serde(crate = "near_sdk::serde")]
    struct D {
        agent_id: String,
    }

    #[test]
    fn sobre_exacto() {
        let d = D {
            agent_id: "7".into(),
        };
        assert_eq!(
            event_log(STANDARD, VERSION, "registered", &d),
            r#"EVENT_JSON:{"standard":"erc8004","version":"1.0.0","event":"registered","data":[{"agent_id":"7"}]}"#
        );
    }
}
