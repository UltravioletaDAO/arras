//! Los metodos de ERC-8004 que escriben: `register`, `set_agent_uri`, `set_metadata`, `set_agent_wallet`,
//! `accept_agent_wallet` y `unset_agent_wallet`. Quien actua es siempre el `predecessor`.

use crate::events::{
    NftMintData, RegisteredData, UriUpdatedData, emit_agent_wallet, emit_metadata_set,
};
use crate::*;
use erc8004_common::events::{emit, emit_nep171};
use erc8004_common::validate::{is_valid_metadata_key, is_valid_optional_uri, parse_u64};
use near_sdk::assert_one_yocto;
use near_sdk::json_types::Base64VecU8;

/// Una entrada de `register(agent_uri, metadata)` (`MetadataEntry` de EVM).
#[near(serializers = [json])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetadataEntry {
    pub metadata_key: String,
    pub metadata_value: Base64VecU8,
}

fn check_uri(uri: &str) {
    if !is_valid_optional_uri(uri, MAX_AGENT_URI_LEN) {
        IdentityError::E01AgentUriInvalida.panic();
    }
}

fn check_entry(key: &str, value: &[u8]) {
    if !is_valid_metadata_key(key) {
        IdentityError::E02ClaveInvalida.panic();
    }
    if key == AGENT_WALLET_KEY {
        IdentityError::E04ClaveReservada.panic();
    }
    if value.len() > MAX_METADATA_VALUE_LEN {
        IdentityError::E05ValorLargo.panic();
    }
}

#[near]
impl IdentityRegistry {
    /// `register()`, `register(agentURI)` y `register(agentURI, metadata[])` en uno. Acuna el agente para el que
    /// llama, con `agentWallet` = el que llama. Adjuntar lo que crece mas la reserva del token (peor caso en
    /// `get_config.register_deposit`); vuelve el sobrante. Devuelve el `agent_id`.
    #[payable]
    pub fn register(
        &mut self,
        agent_uri: Option<String>,
        metadata: Option<Vec<MetadataEntry>>,
    ) -> String {
        let owner = env::predecessor_account_id();
        let uri = agent_uri.unwrap_or_default();
        check_uri(&uri);
        let entries = metadata.unwrap_or_default();
        if entries.len() > MAX_REGISTER_METADATA {
            IdentityError::E03DemasiadaMetadata.panic();
        }
        for e in &entries {
            check_entry(&e.metadata_key, &e.metadata_value.0);
        }
        let deposit = require_deposit();
        let before = self.usage();
        let id = self.next_id;
        self.next_id = id.checked_add(1).unwrap_or_else(|| env::abort());
        let pos = self.add_to_owner(&owner, id);
        self.tokens.insert(
            id,
            Token {
                owner: FixedAccount::of(&owner),
                owner_pos: pos,
                agent_wallet: FixedAccount::of(&owner),
                pending_wallet: FixedAccount::NONE,
                pending_deadline: 0,
                next_approval_id: 0,
                agent_uri: uri.clone(),
            },
        );
        for e in &entries {
            if e.metadata_value.0.is_empty() {
                self.metadata.remove(&(id, e.metadata_key.clone()));
            } else {
                self.metadata
                    .insert((id, e.metadata_key.clone()), e.metadata_value.0.clone());
            }
        }
        let after = self.usage();
        self.settle(before, after, TOKEN_RESERVE_BYTES, &owner, &owner, deposit);
        emit_nep171(
            "nft_mint",
            &NftMintData {
                owner_id: owner.clone(),
                token_ids: vec![id.to_string()],
            },
        );
        emit(
            "registered",
            &RegisteredData {
                agent_id: id.to_string(),
                agent_uri: uri,
                owner: owner.clone(),
            },
        );
        emit_agent_wallet(id, Some(&owner));
        for e in entries {
            emit_metadata_set(id, &e.metadata_key, e.metadata_value.0);
        }
        id.to_string()
    }

    /// `setAgentURI`: el dueno o un aprobado. `""` deja el agente sin URI. Al menos 1 yocto, mas lo que crezca.
    #[payable]
    pub fn set_agent_uri(&mut self, agent_id: String, new_uri: String) {
        let deposit = require_deposit();
        let caller = env::predecessor_account_id();
        let (id, mut token) = self.token_or_panic(&agent_id);
        self.require_authorized(id, &token, &caller);
        check_uri(&new_uri);
        let owner = token.owner.get().unwrap_or_else(|| env::abort());
        let before = self.usage();
        token.agent_uri = new_uri.clone();
        self.tokens.insert(id, token);
        let after = self.usage();
        self.settle(before, after, 0, &caller, &owner, deposit);
        emit(
            "uri_updated",
            &UriUpdatedData {
                agent_id: id.to_string(),
                new_uri,
                updated_by: caller,
            },
        );
    }

    /// `setMetadata`: el dueno o un aprobado; `agentWallet` no (`E04`). Un valor vacio borra la clave. Al menos 1
    /// yocto, mas lo que crezca.
    #[payable]
    pub fn set_metadata(
        &mut self,
        agent_id: String,
        metadata_key: String,
        metadata_value: Base64VecU8,
    ) {
        let deposit = require_deposit();
        let caller = env::predecessor_account_id();
        let (id, token) = self.token_or_panic(&agent_id);
        self.require_authorized(id, &token, &caller);
        check_entry(&metadata_key, &metadata_value.0);
        let owner = token.owner.get().unwrap_or_else(|| env::abort());
        let before = self.usage();
        if metadata_value.0.is_empty() {
            self.metadata.remove(&(id, metadata_key.clone()));
        } else {
            self.metadata
                .insert((id, metadata_key.clone()), metadata_value.0.clone());
        }
        let after = self.usage();
        self.settle(before, after, 0, &caller, &owner, deposit);
        emit_metadata_set(id, &metadata_key, metadata_value.0);
    }

    /// `setAgentWallet` sin firma (desviacion I-1): el dueno o un aprobado, con `now <= deadline <= now + 5 min`
    /// (nanosegundos). Si `new_wallet` es el que llama, queda puesta y devuelve `true`; si no, queda pendiente
    /// (pisa la anterior) hasta que `new_wallet` llame `accept_agent_wallet`, y devuelve `false`. Exactamente 1 yocto.
    #[payable]
    pub fn set_agent_wallet(
        &mut self,
        agent_id: String,
        new_wallet: AccountId,
        deadline: String,
    ) -> bool {
        assert_one_yocto();
        let caller = env::predecessor_account_id();
        let (id, mut token) = self.token_or_panic(&agent_id);
        self.require_authorized(id, &token, &caller);
        let deadline =
            parse_u64(&deadline).unwrap_or_else(|| IdentityError::E13DeadlineInvalido.panic());
        let now = env::block_timestamp();
        if now > deadline {
            IdentityError::E14DeadlineVencido.panic();
        }
        if deadline > now.saturating_add(MAX_DEADLINE_DELAY_NS) {
            IdentityError::E15DeadlineLejos.panic();
        }
        let set_now = caller == new_wallet;
        if set_now {
            token.agent_wallet = FixedAccount::of(&new_wallet);
            token.pending_wallet = FixedAccount::NONE;
            token.pending_deadline = 0;
        } else {
            token.pending_wallet = FixedAccount::of(&new_wallet);
            token.pending_deadline = deadline;
        }
        self.tokens.insert(id, token);
        if set_now {
            emit_agent_wallet(id, Some(&new_wallet));
        }
        set_now
    }

    /// La wallet pendiente acepta (la prueba es que llama ella misma), antes de su `deadline`. Exactamente 1 yocto.
    #[payable]
    pub fn accept_agent_wallet(&mut self, agent_id: String) {
        assert_one_yocto();
        let caller = env::predecessor_account_id();
        let (id, mut token) = self.token_or_panic(&agent_id);
        if token.pending_wallet.is_none() {
            IdentityError::E16SinWalletPendiente.panic();
        }
        if !token.pending_wallet.is(&caller) {
            IdentityError::E17NoEsLaWalletPendiente.panic();
        }
        if env::block_timestamp() > token.pending_deadline {
            IdentityError::E14DeadlineVencido.panic();
        }
        token.agent_wallet = FixedAccount::of(&caller);
        token.pending_wallet = FixedAccount::NONE;
        token.pending_deadline = 0;
        self.tokens.insert(id, token);
        emit_agent_wallet(id, Some(&caller));
    }

    /// `unsetAgentWallet`: el dueno o un aprobado; borra tambien la pendiente. Exactamente 1 yocto.
    #[payable]
    pub fn unset_agent_wallet(&mut self, agent_id: String) {
        assert_one_yocto();
        let caller = env::predecessor_account_id();
        let (id, mut token) = self.token_or_panic(&agent_id);
        self.require_authorized(id, &token, &caller);
        token.agent_wallet = FixedAccount::NONE;
        token.pending_wallet = FixedAccount::NONE;
        token.pending_deadline = 0;
        self.tokens.insert(id, token);
        emit_agent_wallet(id, None);
    }
}
