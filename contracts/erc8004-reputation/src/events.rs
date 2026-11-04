//! Eventos `erc8004` 1.0.0 del Reputation Registry: `new_feedback`, `feedback_revoked`, `response_appended` (seccion 5
//! del plan). Enteros grandes como string; `""` = sin tag/endpoint/URI (como en EVM); hashes en hex o `null`.

use near_sdk::json_types::U128;
use near_sdk::serde::Serialize;
use near_sdk::{AccountId, near};

/// La prueba de `give_feedback_with_proof` en el evento (en `give_feedback`, `null`).
#[near(serializers = [json])]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofData {
    pub verifier: AccountId,
    pub proof: String,
    pub counterparty: AccountId,
    pub role: String,
    pub context: String,
    pub weight: U128,
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(crate = "near_sdk::serde")]
pub struct NewFeedbackData {
    pub agent_id: String,
    pub client_address: AccountId,
    pub feedback_index: String,
    pub value: String,
    pub value_decimals: u8,
    pub tag1: String,
    pub tag2: String,
    pub endpoint: String,
    pub feedback_uri: String,
    pub feedback_hash: Option<String>,
    pub proof: Option<ProofData>,
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(crate = "near_sdk::serde")]
pub struct FeedbackRevokedData {
    pub agent_id: String,
    pub client_address: AccountId,
    pub feedback_index: String,
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(crate = "near_sdk::serde")]
pub struct ResponseAppendedData {
    pub agent_id: String,
    pub client_address: AccountId,
    pub feedback_index: String,
    pub responder: AccountId,
    pub response_uri: String,
    pub response_hash: Option<String>,
}
