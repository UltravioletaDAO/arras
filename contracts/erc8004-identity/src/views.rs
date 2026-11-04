//! Vistas: NEP-171/177/178/181 y las de ERC-8004. Un `agent_id`/`token_id` que no es canonico se trata como un agente
//! que no existe (`null`), salvo en `is_authorized_or_owner`, que entra en panico como el revert de EVM.

use crate::*;
use erc8004_common::types::AgentCheck;
use erc8004_common::validate::parse_u64;
use near_sdk::json_types::{Base64VecU8, U64, U128};
use near_sdk::serde::Serialize;

/// NEP-177, por token. Solo `reference` (= el agentURI) tiene valor; lo demas va en el registration file.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(crate = "near_sdk::serde")]
pub struct TokenMetadata {
    pub title: Option<String>,
    pub description: Option<String>,
    pub media: Option<String>,
    pub media_hash: Option<Base64VecU8>,
    pub copies: Option<u64>,
    pub issued_at: Option<String>,
    pub expires_at: Option<String>,
    pub starts_at: Option<String>,
    pub updated_at: Option<String>,
    pub extra: Option<String>,
    pub reference: Option<String>,
    pub reference_hash: Option<Base64VecU8>,
}

/// NEP-171 `Token` (con NEP-177 y NEP-178).
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(crate = "near_sdk::serde")]
pub struct JsonToken {
    pub token_id: String,
    pub owner_id: AccountId,
    pub metadata: TokenMetadata,
    pub approved_account_ids: BTreeMap<AccountId, u64>,
}

/// NEP-177, del contrato.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(crate = "near_sdk::serde")]
pub struct NftContractMetadata {
    pub spec: String,
    pub name: String,
    pub symbol: String,
    pub icon: Option<String>,
    pub base_uri: Option<String>,
    pub reference: Option<String>,
    pub reference_hash: Option<Base64VecU8>,
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(crate = "near_sdk::serde")]
pub struct PendingWalletView {
    pub wallet: AccountId,
    /// Nanosegundos.
    pub deadline: U64,
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(crate = "near_sdk::serde")]
pub struct IdentityLimits {
    pub max_agent_uri_len: u64,
    pub max_metadata_key_len: u64,
    pub max_metadata_value_len: u64,
    pub max_register_metadata: u64,
    pub max_approvals: u64,
    pub max_memo_len: u64,
    pub max_deadline_delay_ns: U64,
    pub max_list_limit: u64,
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(crate = "near_sdk::serde")]
pub struct IdentityConfig {
    /// `"erc8004-identity"`.
    pub contract: String,
    /// La version de la implementacion de referencia que porta (`getVersion()`).
    pub erc8004_version: String,
    pub events_standard: String,
    pub events_version: String,
    pub total_agents: U64,
    /// Lo que conviene adjuntar a `register` para el peor caso (se cobra lo medido).
    pub register_deposit: U128,
    /// Bytes que `register` cobra de mas por agente y no devuelve (seccion 2.4 del plan).
    pub token_reserve_bytes: u64,
    pub storage_byte_cost: U128,
    pub min_gas_nft_transfer_call: U64,
    pub limits: IdentityLimits,
}

impl IdentityRegistry {
    fn json_token(&self, id: u64, t: &Token) -> JsonToken {
        JsonToken {
            token_id: id.to_string(),
            owner_id: t.owner.get().unwrap_or_else(|| env::abort()),
            metadata: TokenMetadata {
                title: None,
                description: None,
                media: None,
                media_hash: None,
                copies: None,
                issued_at: None,
                expires_at: None,
                starts_at: None,
                updated_at: None,
                extra: None,
                reference: (!t.agent_uri.is_empty()).then(|| t.agent_uri.clone()),
                reference_hash: None,
            },
            approved_account_ids: self.approvals.get(&id).cloned().unwrap_or_default(),
        }
    }

    fn lookup(&self, agent_id: &str) -> Option<(u64, &Token)> {
        let id = parse_u64(agent_id)?;
        self.tokens.get(&id).map(|t| (id, t))
    }
}

fn page(from_index: Option<U128>, limit: Option<u64>) -> (u128, u64) {
    let from = from_index.map(|f| f.0).unwrap_or(0);
    let limit = limit.unwrap_or(MAX_LIST_LIMIT).min(MAX_LIST_LIMIT);
    (from, limit)
}

#[near]
impl IdentityRegistry {
    /// NEP-171.
    pub fn nft_token(&self, token_id: String) -> Option<JsonToken> {
        self.lookup(&token_id).map(|(id, t)| self.json_token(id, t))
    }

    /// NEP-178.
    pub fn nft_is_approved(
        &self,
        token_id: String,
        approved_account_id: AccountId,
        approval_id: Option<u64>,
    ) -> bool {
        let Some((id, _)) = self.lookup(&token_id) else {
            return false;
        };
        match self
            .approvals
            .get(&id)
            .and_then(|m| m.get(&approved_account_id))
        {
            None => false,
            Some(&actual) => approval_id.is_none_or(|expected| expected == actual),
        }
    }

    /// NEP-177.
    pub fn nft_metadata(&self) -> NftContractMetadata {
        NftContractMetadata {
            spec: NFT_METADATA_SPEC.into(),
            name: NFT_NAME.into(),
            symbol: NFT_SYMBOL.into(),
            icon: None,
            base_uri: None,
            reference: None,
            reference_hash: None,
        }
    }

    /// NEP-181.
    pub fn nft_total_supply(&self) -> U128 {
        U128(u128::from(self.next_id))
    }

    /// NEP-181: en orden de `agent_id`. `limit` <= 100.
    pub fn nft_tokens(&self, from_index: Option<U128>, limit: Option<u64>) -> Vec<JsonToken> {
        let (from, limit) = page(from_index, limit);
        let end = u128::from(self.next_id).min(from.saturating_add(u128::from(limit)));
        (from..end)
            .filter_map(|i| {
                let id = i as u64;
                self.tokens.get(&id).map(|t| self.json_token(id, t))
            })
            .collect()
    }

    /// NEP-181.
    pub fn nft_supply_for_owner(&self, account_id: AccountId) -> U128 {
        U128(u128::from(self.owner_supply(&account_id)))
    }

    /// NEP-181: los del dueno, en el orden de su indice. `limit` <= 100.
    pub fn nft_tokens_for_owner(
        &self,
        account_id: AccountId,
        from_index: Option<U128>,
        limit: Option<u64>,
    ) -> Vec<JsonToken> {
        let (from, limit) = page(from_index, limit);
        let n = u128::from(self.owner_supply(&account_id));
        let end = n.min(from.saturating_add(u128::from(limit)));
        (from..end)
            .filter_map(|pos| {
                let id = self.owner_token_at(&account_id, pos as u32)?;
                self.tokens.get(&id).map(|t| self.json_token(id, t))
            })
            .collect()
    }

    /// `tokenURI`: `""` si el agente no tiene URI; `null` si no existe.
    pub fn token_uri(&self, agent_id: String) -> Option<String> {
        self.lookup(&agent_id).map(|(_, t)| t.agent_uri.clone())
    }

    /// `getMetadata`: base64, o `null` si no hay. `agentWallet` devuelve los bytes UTF-8 de la cuenta.
    pub fn get_metadata(&self, agent_id: String, metadata_key: String) -> Option<Base64VecU8> {
        let (id, t) = self.lookup(&agent_id)?;
        if metadata_key == AGENT_WALLET_KEY {
            return t
                .agent_wallet
                .get()
                .map(|w| Base64VecU8(w.as_bytes().to_vec()));
        }
        self.metadata
            .get(&(id, metadata_key))
            .map(|v| Base64VecU8(v.clone()))
    }

    /// `getAgentWallet`: la cuenta, o `null` (el cero de EVM).
    pub fn get_agent_wallet(&self, agent_id: String) -> Option<AccountId> {
        self.lookup(&agent_id)
            .and_then(|(_, t)| t.agent_wallet.get())
    }

    /// La `agentWallet` pendiente de `accept_agent_wallet`, o `null`.
    pub fn get_pending_agent_wallet(&self, agent_id: String) -> Option<PendingWalletView> {
        let (_, t) = self.lookup(&agent_id)?;
        t.pending_wallet.get().map(|wallet| PendingWalletView {
            wallet,
            deadline: U64(t.pending_deadline),
        })
    }

    /// `isAuthorizedOrOwner(spender, agentId)`: entra en panico si el agente no existe (como el revert
    /// `ERC721NonexistentToken`).
    pub fn is_authorized_or_owner(&self, spender: AccountId, agent_id: String) -> bool {
        let (id, t) = self.token_or_panic(&agent_id);
        self.is_authorized(id, &t, &spender)
    }

    /// Para las llamadas cruzadas de Reputation y Validation: `null` si el agente no existe; si existe, su dueno, su
    /// `agentWallet` y si `account_id` es el dueno o un aprobado. Nunca entra en panico.
    pub fn check_agent(&self, agent_id: String, account_id: AccountId) -> Option<AgentCheck> {
        let (id, t) = self.lookup(&agent_id)?;
        Some(AgentCheck {
            owner_id: t.owner.get().unwrap_or_else(|| env::abort()),
            agent_wallet: t.agent_wallet.get(),
            authorized: self.is_authorized(id, t, &account_id),
        })
    }

    /// `getVersion()`.
    pub fn get_version(&self) -> String {
        erc8004_common::ERC8004_VERSION.into()
    }

    pub fn get_config(&self) -> IdentityConfig {
        IdentityConfig {
            contract: "erc8004-identity".into(),
            erc8004_version: erc8004_common::ERC8004_VERSION.into(),
            events_standard: erc8004_common::events::STANDARD.into(),
            events_version: erc8004_common::events::VERSION.into(),
            total_agents: U64(self.next_id),
            register_deposit: U128(bytes_cost(REGISTER_MAX_BYTES)),
            token_reserve_bytes: TOKEN_RESERVE_BYTES,
            storage_byte_cost: U128(env::storage_byte_cost().as_yoctonear()),
            min_gas_nft_transfer_call: U64(MIN_GAS_NFT_TRANSFER_CALL.as_gas()),
            limits: IdentityLimits {
                max_agent_uri_len: MAX_AGENT_URI_LEN as u64,
                max_metadata_key_len: erc8004_common::validate::MAX_METADATA_KEY_LEN as u64,
                max_metadata_value_len: MAX_METADATA_VALUE_LEN as u64,
                max_register_metadata: MAX_REGISTER_METADATA as u64,
                max_approvals: MAX_APPROVALS as u64,
                max_memo_len: MAX_MEMO_LEN as u64,
                max_deadline_delay_ns: U64(MAX_DEADLINE_DELAY_NS),
                max_list_limit: MAX_LIST_LIMIT,
            },
        }
    }
}
