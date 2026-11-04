//! Eventos del Identity Registry: `erc8004` 1.0.0 (`registered`, `metadata_set`, `uri_updated` y
//! `agent_wallet_pending`) y `nep171` 1.0.0 (`nft_mint`, `nft_transfer`). Ver la seccion 5 del plan.

use near_sdk::AccountId;
use near_sdk::json_types::{Base64VecU8, U64};
use near_sdk::serde::Serialize;

/// `nft_mint` (NEP-171).
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(crate = "near_sdk::serde")]
pub struct NftMintData {
    pub owner_id: AccountId,
    pub token_ids: Vec<String>,
}

/// `nft_transfer` (NEP-171). `authorized_id` solo si transfirio un aprobado.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(crate = "near_sdk::serde")]
pub struct NftTransferData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorized_id: Option<AccountId>,
    pub old_owner_id: AccountId,
    pub new_owner_id: AccountId,
    pub token_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo: Option<String>,
}

/// `registered` (`Registered(agentId, agentURI, owner)`).
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(crate = "near_sdk::serde")]
pub struct RegisteredData {
    pub agent_id: String,
    pub agent_uri: String,
    pub owner: AccountId,
}

/// `metadata_set` (`MetadataSet(agentId, key, key, value)`): `metadata_value` en base64 (`""` = borrado).
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(crate = "near_sdk::serde")]
pub struct MetadataSetData {
    pub agent_id: String,
    pub metadata_key: String,
    pub metadata_value: Base64VecU8,
}

/// `uri_updated` (`URIUpdated(agentId, newURI, updatedBy)`).
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(crate = "near_sdk::serde")]
pub struct UriUpdatedData {
    pub agent_id: String,
    pub new_uri: String,
    pub updated_by: AccountId,
}

/// `agent_wallet_pending` (sin equivalente en EVM, desviacion I-9): `set_agent_wallet` dejo `wallet` pendiente de
/// `accept_agent_wallet` hasta `deadline` (ns), o una revocacion (`nft_revoke`, `nft_revoke_all`) la cancelo
/// (`wallet` null, `deadline` `"0"`).
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(crate = "near_sdk::serde")]
pub struct AgentWalletPendingData {
    pub agent_id: String,
    pub wallet: Option<AccountId>,
    pub deadline: U64,
}

pub(crate) fn emit_agent_wallet_pending(agent_id: u64, wallet: Option<&AccountId>, deadline: u64) {
    erc8004_common::events::emit(
        "agent_wallet_pending",
        &AgentWalletPendingData {
            agent_id: agent_id.to_string(),
            wallet: wallet.cloned(),
            deadline: U64(deadline),
        },
    );
}

pub(crate) fn emit_metadata_set(agent_id: u64, key: &str, value: Vec<u8>) {
    erc8004_common::events::emit(
        "metadata_set",
        &MetadataSetData {
            agent_id: agent_id.to_string(),
            metadata_key: key.to_string(),
            metadata_value: Base64VecU8(value),
        },
    );
}

/// `metadata_set` de `agentWallet`: los bytes UTF-8 de la cuenta (vacio = limpia).
pub(crate) fn emit_agent_wallet(agent_id: u64, wallet: Option<&AccountId>) {
    emit_metadata_set(
        agent_id,
        crate::AGENT_WALLET_KEY,
        wallet.map(|w| w.as_bytes().to_vec()).unwrap_or_default(),
    );
}
