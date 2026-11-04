//! Eventos NEP-297 del registro. `standard` = `arras-registry`, `version` = `1.0.0`. `data` siempre es un arreglo de
//! un elemento (como los eventos `arras` de escrow-core).
//!
//! Formato exacto: `EVENT_JSON:{"standard":"arras-registry","version":"1.0.0","event":"<nombre>","data":[{...}]}`.
//! Nombres: `agent_registered`, `agent_updated`, `feedback_given` y `feedback_revoked`.

use near_sdk::json_types::{U64, U128};
use near_sdk::serde::Serialize;
use near_sdk::{AccountId, near};

use crate::Role;

/// `standard` de los eventos del registro.
pub const EVENT_STANDARD: &str = "arras-registry";
/// `version` de los eventos del registro v1.
pub const EVENT_VERSION: &str = "1.0.0";

/// `agent_registered` y `agent_updated`: la ficha completa despues del cambio (`metadata` sale `null` si no hay).
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(crate = "near_sdk::serde")]
pub struct AgentData {
    pub account_id: AccountId,
    pub agent_uri: String,
    pub metadata: Option<String>,
    pub at: U64,
}

/// `feedback_given`: la calificacion tal como queda guardada.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(crate = "near_sdk::serde")]
pub struct FeedbackGivenData {
    pub feedback_id: u32,
    pub rater: AccountId,
    pub ratee: AccountId,
    /// El rol de la calificada en el pago: `payer` (el agente que pago) o `receiver` (el worker que cobro).
    pub ratee_role: Role,
    pub escrow: AccountId,
    pub payment_id: String,
    pub operator_id: String,
    /// `released` del pago al momento de calificar.
    pub released: U128,
    pub score: u8,
    pub tag1: Option<String>,
    pub tag2: Option<String>,
    pub feedback_uri: Option<String>,
    /// 64 hex en minusculas o `null`.
    pub feedback_hash: Option<String>,
    pub created_at: U64,
}

/// `feedback_revoked`.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(crate = "near_sdk::serde")]
pub struct FeedbackRevokedData {
    pub feedback_id: u32,
    pub rater: AccountId,
    pub ratee: AccountId,
    pub escrow: AccountId,
    pub payment_id: String,
    pub revoked_at: U64,
}

#[near(event_json(standard = "arras-registry"))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryEvent {
    #[event_version("1.0.0")]
    AgentRegistered(Vec<AgentData>),
    #[event_version("1.0.0")]
    AgentUpdated(Vec<AgentData>),
    #[event_version("1.0.0")]
    FeedbackGiven(Vec<FeedbackGivenData>),
    #[event_version("1.0.0")]
    FeedbackRevoked(Vec<FeedbackRevokedData>),
}

/// El log completo, `EVENT_JSON:{...}`, sin emitirlo (para tests y para el SDK).
pub fn event_log(e: &RegistryEvent) -> String {
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
    fn formato_exacto_de_cada_evento() {
        let alta = RegistryEvent::AgentRegistered(vec![AgentData {
            account_id: acc("agente.near"),
            agent_uri: "https://agente.example/card.json".into(),
            metadata: None,
            at: U64(5),
        }]);
        assert_eq!(
            event_log(&alta),
            r#"EVENT_JSON:{"standard":"arras-registry","version":"1.0.0","event":"agent_registered","data":[{"account_id":"agente.near","agent_uri":"https://agente.example/card.json","metadata":null,"at":"5"}]}"#
        );
        let cambio = RegistryEvent::AgentUpdated(vec![AgentData {
            account_id: acc("agente.near"),
            agent_uri: "ipfs://bafy".into(),
            metadata: Some(r#"{"k":"v"}"#.into()),
            at: U64(6),
        }]);
        assert_eq!(
            event_log(&cambio),
            r#"EVENT_JSON:{"standard":"arras-registry","version":"1.0.0","event":"agent_updated","data":[{"account_id":"agente.near","agent_uri":"ipfs://bafy","metadata":"{\"k\":\"v\"}","at":"6"}]}"#
        );
        let dada = RegistryEvent::FeedbackGiven(vec![FeedbackGivenData {
            feedback_id: 0,
            rater: acc("payer.near"),
            ratee: acc("worker.near"),
            ratee_role: Role::Receiver,
            escrow: acc("v1.arras-escrow.near"),
            payment_id: "DN5eMA3sNES7xsoy6qvy24cVF6Dmwjxpkvd2b4e8P38M".into(),
            operator_id: "em".into(),
            released: U128(1_000_000),
            score: 90,
            tag1: Some("quality".into()),
            tag2: None,
            feedback_uri: None,
            feedback_hash: None,
            created_at: U64(7),
        }]);
        assert_eq!(
            event_log(&dada),
            r#"EVENT_JSON:{"standard":"arras-registry","version":"1.0.0","event":"feedback_given","data":[{"feedback_id":0,"rater":"payer.near","ratee":"worker.near","ratee_role":"receiver","escrow":"v1.arras-escrow.near","payment_id":"DN5eMA3sNES7xsoy6qvy24cVF6Dmwjxpkvd2b4e8P38M","operator_id":"em","released":"1000000","score":90,"tag1":"quality","tag2":null,"feedback_uri":null,"feedback_hash":null,"created_at":"7"}]}"#
        );
        let revocada = RegistryEvent::FeedbackRevoked(vec![FeedbackRevokedData {
            feedback_id: 0,
            rater: acc("payer.near"),
            ratee: acc("worker.near"),
            escrow: acc("v1.arras-escrow.near"),
            payment_id: "x".into(),
            revoked_at: U64(8),
        }]);
        assert_eq!(
            event_log(&revocada),
            r#"EVENT_JSON:{"standard":"arras-registry","version":"1.0.0","event":"feedback_revoked","data":[{"feedback_id":0,"rater":"payer.near","ratee":"worker.near","escrow":"v1.arras-escrow.near","payment_id":"x","revoked_at":"8"}]}"#
        );
        assert_eq!(alta.standard(), EVENT_STANDARD);
        assert_eq!(alta.version(), EVENT_VERSION);
    }
}
