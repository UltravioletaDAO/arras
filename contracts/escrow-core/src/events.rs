//! Eventos NEP-297 (sec. 3.7). `standard` = `arras`, `version` = `1.0.0`. `data` siempre es un arreglo: cada
//! variante lleva `Vec<Dato>` (con una variante struct, `data` saldria como objeto).
//!
//! Formato exacto: `EVENT_JSON:{"standard":"arras","version":"1.0.0","event":"<nombre>","data":[{...}]}`.

use near_sdk::json_types::{U64, U128};
use near_sdk::serde::Serialize;
use near_sdk::{AccountId, near};

/// `standard` de los eventos de Arras. Queda on-chain para siempre: no lleva el nombre de ningun otro producto.
pub const EVENT_STANDARD: &str = "arras";
/// `version` de los eventos de v1.
pub const EVENT_VERSION: &str = "1.0.0";

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(crate = "near_sdk::serde")]
pub struct AuthorizedData {
    pub payment_id: String,
    pub operator_id: String,
    pub token: AccountId,
    pub payer: AccountId,
    pub receiver: AccountId,
    pub amount: U128,
    pub fee_bps: u16,
    pub fee_recipient: AccountId,
    pub terms_hash: String,
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(crate = "near_sdk::serde")]
pub struct ReleasedData {
    pub payment_id: String,
    pub amount: U128,
    pub net: U128,
    pub fee: U128,
}

/// `refunded` y `reclaimed`.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(crate = "near_sdk::serde")]
pub struct PaymentAmountData {
    pub payment_id: String,
    pub amount: U128,
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(crate = "near_sdk::serde")]
pub struct WithdrawnData {
    pub account: AccountId,
    pub token: AccountId,
    pub amount: U128,
    pub ok: bool,
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(crate = "near_sdk::serde")]
pub struct CompactedData {
    pub payment_id: String,
}

/// `operator_registered` (todos los campos) y `operator_updated` (solo los que cambiaron).
#[derive(Serialize, Debug, Clone, PartialEq, Eq, Default)]
#[serde(crate = "near_sdk::serde")]
pub struct OperatorData {
    pub operator_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<AccountId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capturers: Option<Vec<AccountId>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee_bps: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee_recipient: Option<AccountId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_amount: Option<U128>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tope_tvl_operador: Option<U128>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tope_tvl_por_payer: Option<U128>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sponsor_max_por_payer: Option<U128>,
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(crate = "near_sdk::serde")]
pub struct CapsSetData {
    pub by: AccountId,
    pub tope_por_pago: U128,
    pub tope_tvl_global: U128,
}

/// `paused` y `unpaused`.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(crate = "near_sdk::serde")]
pub struct PauseData {
    pub by: AccountId,
    pub paused: bool,
    pub at: U64,
}

#[near(event_json(standard = "arras"))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArrasEvent {
    #[event_version("1.0.0")]
    Authorized(Vec<AuthorizedData>),
    #[event_version("1.0.0")]
    Released(Vec<ReleasedData>),
    #[event_version("1.0.0")]
    Refunded(Vec<PaymentAmountData>),
    #[event_version("1.0.0")]
    Reclaimed(Vec<PaymentAmountData>),
    #[event_version("1.0.0")]
    Withdrawn(Vec<WithdrawnData>),
    #[event_version("1.0.0")]
    Compacted(Vec<CompactedData>),
    #[event_version("1.0.0")]
    OperatorRegistered(Vec<OperatorData>),
    #[event_version("1.0.0")]
    OperatorUpdated(Vec<OperatorData>),
    #[event_version("1.0.0")]
    CapsSet(Vec<CapsSetData>),
    #[event_version("1.0.0")]
    Paused(Vec<PauseData>),
    #[event_version("1.0.0")]
    Unpaused(Vec<PauseData>),
}

/// El log completo, `EVENT_JSON:{...}`, sin emitirlo (para tests y para el SDK).
pub fn event_log(e: &ArrasEvent) -> String {
    format!(
        "EVENT_JSON:{}",
        near_sdk::serde_json::to_string(&e.to_json()).unwrap_or_default()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn acc(s: &str) -> AccountId {
        s.parse().unwrap()
    }

    #[test]
    fn formato_exacto_nep297_con_data_arreglo() {
        let e = ArrasEvent::Released(vec![ReleasedData {
            payment_id: "DN5eMA3sNES7xsoy6qvy24cVF6Dmwjxpkvd2b4e8P38M".into(),
            amount: U128(1_000_000),
            net: U128(870_000),
            fee: U128(130_000),
        }]);
        assert_eq!(
            event_log(&e),
            r#"EVENT_JSON:{"standard":"arras","version":"1.0.0","event":"released","data":[{"payment_id":"DN5eMA3sNES7xsoy6qvy24cVF6Dmwjxpkvd2b4e8P38M","amount":"1000000","net":"870000","fee":"130000"}]}"#
        );
        assert_eq!(e.standard(), EVENT_STANDARD);
        assert_eq!(e.version(), EVENT_VERSION);
    }

    #[test]
    fn nombres_de_eventos_en_snake_case() {
        let by = acc("owner.testnet");
        let p = PauseData {
            by: by.clone(),
            paused: true,
            at: U64(0),
        };
        let casos = [
            (
                ArrasEvent::Withdrawn(vec![WithdrawnData {
                    account: by.clone(),
                    token: by.clone(),
                    amount: U128(1),
                    ok: false,
                }]),
                "withdrawn",
            ),
            (
                ArrasEvent::Compacted(vec![CompactedData {
                    payment_id: "x".into(),
                }]),
                "compacted",
            ),
            (
                ArrasEvent::OperatorRegistered(vec![OperatorData {
                    operator_id: "em".into(),
                    ..Default::default()
                }]),
                "operator_registered",
            ),
            (
                ArrasEvent::OperatorUpdated(vec![OperatorData {
                    operator_id: "em".into(),
                    fee_bps: Some(1800),
                    ..Default::default()
                }]),
                "operator_updated",
            ),
            (
                ArrasEvent::CapsSet(vec![CapsSetData {
                    by: by.clone(),
                    tope_por_pago: U128(1),
                    tope_tvl_global: U128(2),
                }]),
                "caps_set",
            ),
            (ArrasEvent::Paused(vec![p.clone()]), "paused"),
            (ArrasEvent::Unpaused(vec![p]), "unpaused"),
            (
                ArrasEvent::Reclaimed(vec![PaymentAmountData {
                    payment_id: "x".into(),
                    amount: U128(1),
                }]),
                "reclaimed",
            ),
        ];
        for (e, nombre) in casos {
            let log = event_log(&e);
            assert!(log.starts_with(&format!("EVENT_JSON:{{\"standard\":\"arras\",\"version\":\"1.0.0\",\"event\":\"{nombre}\",\"data\":[{{")), "{log}");
            assert!(log.ends_with("}]}"), "{log}");
        }
        // operator_updated lleva solo lo que cambio.
        let upd = event_log(&ArrasEvent::OperatorUpdated(vec![OperatorData {
            operator_id: "em".into(),
            fee_bps: Some(1800),
            ..Default::default()
        }]));
        assert!(
            upd.ends_with(r#""data":[{"operator_id":"em","fee_bps":1800}]}"#),
            "{upd}"
        );
    }
}
