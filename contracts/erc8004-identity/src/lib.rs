//! ERC-8004 Identity Registry en NEAR (`docs/plans/erc8004-near-port.md`, seccion 2), port de
//! `IdentityRegistryUpgradeable` 2.0.0 (`erc-8004/erc-8004-contracts` b9e466c). Inmutable: `new` privado en la misma
//! tx del deploy, sin `migrate`, sin owner, sin llaves despues.
//!
//! - Cada agente es un NFT (NEP-171 core, NEP-177 metadata, NEP-178 aprobaciones, NEP-181 enumeracion).
//!   `agent_id` = `token_id` = un contador desde `"0"` (como `_lastId++`); `agentURI` = `metadata.reference`.
//! - "Autorizado" = el dueno o una cuenta aprobada por NEP-178 para ese agente (el `getApproved` de ERC-721; no hay
//!   `setApprovalForAll`, desviacion I-3).
//! - Metadata clave -> bytes (`get_metadata` / `set_metadata`), con la clave reservada `agentWallet`.
//! - `agentWallet`: la pone `register` (el que registra), la cambia `set_agent_wallet` (si la wallet nueva es el que
//!   llama, en el acto; si no, queda pendiente hasta que la wallet llame `accept_agent_wallet`), la borra
//!   `unset_agent_wallet` y **toda transferencia** (como `_update` de EVM). La pendiente la cancelan ademas
//!   `nft_revoke` y `nft_revoke_all` (desviacion I-9).
//!
//! Storage (seccion 2.4 del plan): quien llama paga lo que crece; lo liberado vuelve al dueno. Una transferencia (1
//! yocto, NEP-171) nunca puede cobrar: todo lo que cambia en ella es de largo fijo ([`FixedAccount`]) salvo la
//! cantidad de dueños con agentes (un registro de [`OWNER_COUNT_RECORD_BYTES`] por dueno), y `register` cobra ese
//! registro de mas por agente ([`TOKEN_RESERVE_BYTES`]): como nunca hay mas duenos con agentes que agentes, el
//! storage del contrato siempre esta pagado.

use erc8004_common::storage::{bytes_cost, record_bytes, refund, string_bytes};
use near_sdk::store::LookupMap;
use near_sdk::{AccountId, BorshStorageKey, Gas, PanicOnDefault, env, near};
use std::collections::BTreeMap;

mod error;
mod events;
mod nft;
mod registry;
mod views;

pub use error::IdentityError;
pub use events::*;
pub use views::*;

#[cfg(test)]
mod unit_tests;

/// `name()` y `symbol()` del ERC-721 oficial.
pub const NFT_NAME: &str = "AgentIdentity";
pub const NFT_SYMBOL: &str = "AGENT";
/// `spec` de NEP-177.
pub const NFT_METADATA_SPEC: &str = "nft-1.0.0";
/// La clave reservada.
pub const AGENT_WALLET_KEY: &str = "agentWallet";

/// Tope de `agent_uri`: deja subir un registration file entero como `data:` base64 (lo sugiere la EIP).
pub const MAX_AGENT_URI_LEN: usize = 8192;
/// Tope del valor de una entrada de metadata.
pub const MAX_METADATA_VALUE_LEN: usize = 1024;
/// Entradas de metadata en un `register` (el peor caso de logs de `register` tiene que entrar en 16.384 bytes).
pub const MAX_REGISTER_METADATA: usize = 4;
/// Aprobaciones NEP-178 por agente.
pub const MAX_APPROVALS: usize = 8;
/// Tope del `memo` de NEP-171 (va en el log de `nft_transfer`).
pub const MAX_MEMO_LEN: usize = 256;
/// `MAX_DEADLINE_DELAY` de EVM (5 minutos), en nanosegundos.
pub const MAX_DEADLINE_DELAY_NS: u64 = 5 * 60 * 1_000_000_000;
/// Tope (y valor por defecto) de `limit` en las vistas paginadas.
pub const MAX_LIST_LIMIT: u64 = 100;

/// Gas reservado para `nft_resolve_transfer`.
pub const GAS_FOR_RESOLVE_TRANSFER: Gas = Gas::from_tgas(15);
/// Gas minimo que se le da a `nft_on_transfer` del receptor (ademas, todo el gas que sobre).
pub const GAS_FOR_NFT_ON_TRANSFER: Gas = Gas::from_tgas(5);
/// Gas minimo de `nft_transfer_call`.
pub const MIN_GAS_NFT_TRANSFER_CALL: Gas = Gas::from_tgas(50);
/// Gas de `nft_on_approve` cuando `nft_approve` lleva `msg`.
pub const GAS_FOR_NFT_ON_APPROVE: Gas = Gas::from_tgas(10);

/// Claves de storage.
#[derive(BorshStorageKey)]
#[near]
pub enum StorageKey {
    Tokens,
    Metadata,
    Approvals,
    OwnerCount,
    OwnerTokens,
}

/// Una cuenta en 65 bytes fijos (largo + 64): asi cambiar el dueno o la `agentWallet` nunca cambia el storage.
/// `len == 0` es "no hay".
#[near(serializers = [borsh])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FixedAccount {
    len: u8,
    bytes: [u8; 64],
}

impl FixedAccount {
    pub const NONE: Self = Self {
        len: 0,
        bytes: [0; 64],
    };

    pub fn of(a: &AccountId) -> Self {
        let b = a.as_bytes();
        let mut bytes = [0u8; 64];
        bytes[..b.len()].copy_from_slice(b);
        Self {
            len: b.len() as u8,
            bytes,
        }
    }

    pub fn get(&self) -> Option<AccountId> {
        if self.len == 0 {
            return None;
        }
        let s =
            std::str::from_utf8(&self.bytes[..self.len as usize]).unwrap_or_else(|_| env::abort());
        Some(s.parse().unwrap_or_else(|_| env::abort()))
    }

    pub fn is(&self, a: &AccountId) -> bool {
        self.len != 0 && &self.bytes[..self.len as usize] == a.as_bytes()
    }

    pub fn is_none(&self) -> bool {
        self.len == 0
    }
}

/// Un agente.
#[near(serializers = [borsh])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    pub owner: FixedAccount,
    /// Posicion en el indice del dueno (`owner_tokens`).
    pub owner_pos: u32,
    pub agent_wallet: FixedAccount,
    /// La `agentWallet` pendiente de `accept_agent_wallet` y su `deadline` (ns).
    pub pending_wallet: FixedAccount,
    pub pending_deadline: u64,
    /// El proximo `approval_id` de NEP-178: nunca baja (tampoco en una transferencia).
    pub next_approval_id: u64,
    /// `""` = sin URI (`register()` sin argumentos, o `set_agent_uri("")`).
    pub agent_uri: String,
}

/// Bytes de un registro del contador por dueno (`owner_count`: clave 1 + sha256, valor u32).
pub const OWNER_COUNT_RECORD_BYTES: u64 = record_bytes(1 + 32, 4);
/// Lo que `register` cobra de mas por agente y no devuelve: un contador por dueno (seccion 2.4 del plan).
pub const TOKEN_RESERVE_BYTES: u64 = OWNER_COUNT_RECORD_BYTES;
/// Bytes fijos de un `Token` (todo menos `agent_uri`).
pub const TOKEN_FIXED_BYTES: u64 = 65 * 3 + 4 + 8 + 8;
/// Peor caso de `register`: el `Token` con la URI al tope, su entrada en el indice del dueno, el contador de un dueno
/// nuevo, `MAX_REGISTER_METADATA` entradas al tope, y la reserva. Es lo que `get_config` recomienda adjuntar; se cobra
/// lo medido. U-ST1 mide que el peor caso real es exactamente este.
pub const REGISTER_MAX_BYTES: u64 = {
    let token = record_bytes(1 + 8, TOKEN_FIXED_BYTES + string_bytes(MAX_AGENT_URI_LEN));
    let owner_entry = record_bytes(1 + 32 + 4, 8);
    let metadata = record_bytes(
        1 + 8 + string_bytes(erc8004_common::validate::MAX_METADATA_KEY_LEN),
        4 + MAX_METADATA_VALUE_LEN as u64,
    );
    token
        + owner_entry
        + OWNER_COUNT_RECORD_BYTES
        + metadata * MAX_REGISTER_METADATA as u64
        + TOKEN_RESERVE_BYTES
};

/// Contrato `IdentityRegistry`. NEP-330 `standards` (seccion 7 del plan): los NEP del NFT y `erc8004` 1.0.0 (la
/// interfaz y los eventos de este port); `nep330` lo agrega near-sdk, y `link`/`version` los pone cargo-near.
#[near(
    contract_state,
    contract_metadata(
        standard(standard = "nep171", version = "1.0.0"),
        standard(standard = "nep177", version = "1.0.0"),
        standard(standard = "nep178", version = "1.0.0"),
        standard(standard = "nep181", version = "1.0.0"),
        standard(standard = "erc8004", version = "1.0.0")
    )
)]
#[derive(PanicOnDefault)]
pub struct IdentityRegistry {
    /// El proximo `agent_id` (= la cantidad de agentes: no hay burn).
    next_id: u64,
    tokens: LookupMap<u64, Token>,
    /// `(agent_id, clave)` -> valor. Un valor vacio no se guarda.
    metadata: LookupMap<(u64, String), Vec<u8>>,
    /// NEP-178: `agent_id` -> cuenta -> `approval_id`. Sin entrada si no hay aprobaciones.
    approvals: LookupMap<u64, BTreeMap<AccountId, u64>>,
    /// sha256(dueno) -> cuantos agentes tiene. Sin entrada si no tiene ninguno.
    owner_count: LookupMap<[u8; 32], u32>,
    /// `(sha256(dueno), posicion)` -> `agent_id`.
    owner_tokens: LookupMap<([u8; 32], u32), u64>,
}

#[near]
impl IdentityRegistry {
    /// Solo la propia cuenta (con `with-init-call` en el deploy): `#[private]` cierra el `new` ajeno.
    #[init]
    #[private]
    pub fn new() -> Self {
        Self {
            next_id: 0,
            tokens: LookupMap::new(StorageKey::Tokens),
            metadata: LookupMap::new(StorageKey::Metadata),
            approvals: LookupMap::new(StorageKey::Approvals),
            owner_count: LookupMap::new(StorageKey::OwnerCount),
            owner_tokens: LookupMap::new(StorageKey::OwnerTokens),
        }
    }
}

/// `agent_id` (string canonico) -> `u64`, o `E10`.
pub(crate) fn parse_id(agent_id: &str) -> u64 {
    erc8004_common::validate::parse_u64(agent_id)
        .unwrap_or_else(|| IdentityError::E10AgentIdInvalido.panic())
}

/// El deposito adjunto, que tiene que ser de al menos 1 yocto (`E07`).
pub(crate) fn require_deposit() -> u128 {
    let d = env::attached_deposit().as_yoctonear();
    if d == 0 {
        IdentityError::E07RequiereDeposito.panic();
    }
    d
}

fn owner_hash(a: &AccountId) -> [u8; 32] {
    env::sha256_array(a.as_bytes())
}

impl IdentityRegistry {
    pub(crate) fn flush_all(&mut self) {
        self.tokens.flush();
        self.metadata.flush();
        self.approvals.flush();
        self.owner_count.flush();
        self.owner_tokens.flush();
    }

    /// `env::storage_usage()` con todo escrito.
    pub(crate) fn usage(&mut self) -> u64 {
        self.flush_all();
        env::storage_usage()
    }

    /// El agente, o `E10` / `E11`.
    pub(crate) fn token_or_panic(&self, agent_id: &str) -> (u64, Token) {
        let id = parse_id(agent_id);
        match self.tokens.get(&id) {
            Some(t) => (id, t.clone()),
            None => IdentityError::E11AgenteInexistente.panic(),
        }
    }

    /// Dueno o aprobado (NEP-178).
    pub(crate) fn is_authorized(&self, id: u64, token: &Token, who: &AccountId) -> bool {
        token.owner.is(who) || self.approvals.get(&id).is_some_and(|m| m.contains_key(who))
    }

    pub(crate) fn require_authorized(&self, id: u64, token: &Token, who: &AccountId) {
        if !self.is_authorized(id, token, who) {
            IdentityError::E12NoAutorizado.panic();
        }
    }

    /// Cierra el storage de una llamada pagable: lo que crecio lo paga `payer` con `deposit` (o `E06`), y el resto del
    /// deposito vuelve a `payer`; lo que se libero vuelve a `owner` (desviacion S-1).
    pub(crate) fn settle(
        &self,
        before: u64,
        after: u64,
        reserve: u64,
        payer: &AccountId,
        owner: &AccountId,
        deposit: u128,
    ) {
        if after + reserve >= before {
            let cost = bytes_cost(after + reserve - before);
            if deposit < cost {
                IdentityError::E06DepositoInsuficiente(cost).panic();
            }
            refund(payer, deposit - cost);
        } else {
            let freed = bytes_cost(before - after - reserve);
            if payer == owner {
                refund(payer, deposit + freed);
            } else {
                refund(payer, deposit);
                refund(owner, freed);
            }
        }
    }

    /// Agrega `id` al indice de `owner`; devuelve su posicion.
    pub(crate) fn add_to_owner(&mut self, owner: &AccountId, id: u64) -> u32 {
        let h = owner_hash(owner);
        let n = self.owner_count.get(&h).copied().unwrap_or(0);
        self.owner_tokens.insert((h, n), id);
        self.owner_count.insert(h, n + 1);
        n
    }

    /// Saca `id` (en `pos`) del indice de `owner`, moviendo el ultimo a su lugar.
    pub(crate) fn remove_from_owner(&mut self, owner: &AccountId, pos: u32) {
        let h = owner_hash(owner);
        let n = self
            .owner_count
            .get(&h)
            .copied()
            .unwrap_or_else(|| env::abort());
        let last = n - 1;
        if pos != last {
            let moved = self
                .owner_tokens
                .get(&(h, last))
                .copied()
                .unwrap_or_else(|| env::abort());
            self.owner_tokens.insert((h, pos), moved);
            let t = self.tokens.get_mut(&moved).unwrap_or_else(|| env::abort());
            t.owner_pos = pos;
        }
        self.owner_tokens.remove(&(h, last));
        if last == 0 {
            self.owner_count.remove(&h);
        } else {
            self.owner_count.insert(h, last);
        }
    }

    /// Cuantos agentes tiene `owner`.
    pub(crate) fn owner_supply(&self, owner: &AccountId) -> u32 {
        self.owner_count
            .get(&owner_hash(owner))
            .copied()
            .unwrap_or(0)
    }

    /// El `agent_id` en la posicion `pos` del indice de `owner`.
    pub(crate) fn owner_token_at(&self, owner: &AccountId, pos: u32) -> Option<u64> {
        self.owner_tokens.get(&(owner_hash(owner), pos)).copied()
    }
}
