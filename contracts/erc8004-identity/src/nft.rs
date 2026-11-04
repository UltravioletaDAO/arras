//! NEP-171 (transferencias) y NEP-178 (aprobaciones). Toda transferencia limpia `agentWallet` y la pendiente (como
//! `_update` de EVM) y borra las aprobaciones (su storage vuelve al dueno anterior).

use crate::events::{NftTransferData, emit_agent_wallet};
use crate::*;
use erc8004_common::events::emit_nep171;
use erc8004_common::storage::{bytes_cost, refund};
use near_sdk::json_types::U128;
use near_sdk::{Promise, PromiseOrValue, assert_one_yocto, ext_contract, serde_json};

/// El receptor de `nft_transfer_call` (NEP-171).
#[allow(dead_code)]
#[ext_contract(ext_nft_receiver)]
pub trait NonFungibleTokenReceiver {
    fn nft_on_transfer(
        &mut self,
        sender_id: AccountId,
        previous_owner_id: AccountId,
        token_id: String,
        msg: String,
    ) -> PromiseOrValue<bool>;
}

/// El aprobado de `nft_approve` con `msg` (NEP-178).
#[allow(dead_code)]
#[ext_contract(ext_nft_approval_receiver)]
pub trait NonFungibleTokenApprovalReceiver {
    fn nft_on_approve(
        &mut self,
        token_id: String,
        owner_id: AccountId,
        approval_id: u64,
        msg: String,
    ) -> near_sdk::PromiseOrValue<String>;
}

/// Lo que devuelve una transferencia: el dueno anterior, sus aprobaciones (las de NEP-178 que se borraron) y los
/// bytes que liberaron.
pub(crate) struct Transferred {
    pub previous_owner: AccountId,
    pub approvals: Option<BTreeMap<AccountId, u64>>,
    pub freed_bytes: u64,
}

impl IdentityRegistry {
    /// Mueve el agente de su dueno a `receiver`. `sender` es el dueno o un aprobado (con `approval_id` si lo pasa).
    /// No devuelve el storage de las aprobaciones: lo hace quien llama (`nft_transfer` en el acto,
    /// `nft_resolve_transfer` al final).
    pub(crate) fn internal_transfer(
        &mut self,
        sender: &AccountId,
        receiver: &AccountId,
        token_id: &str,
        approval_id: Option<u64>,
        memo: Option<String>,
    ) -> Transferred {
        if memo.as_ref().is_some_and(|m| m.len() > MAX_MEMO_LEN) {
            IdentityError::E18MemoLargo.panic();
        }
        let (id, mut token) = self.token_or_panic(token_id);
        let owner = token.owner.get().unwrap_or_else(|| env::abort());
        if sender != &owner {
            let Some(&actual) = self.approvals.get(&id).and_then(|m| m.get(sender)) else {
                IdentityError::E12NoAutorizado.panic();
            };
            if approval_id.is_some_and(|expected| expected != actual) {
                IdentityError::E19ApprovalIdNoCoincide.panic();
            }
        }
        if receiver == &owner {
            IdentityError::E20ReceptorEsElDueno.panic();
        }
        let before = self.usage();
        let approvals = self.approvals.remove(&id);
        let freed_bytes = before.saturating_sub(self.usage());
        self.remove_from_owner(&owner, token.owner_pos);
        token.owner_pos = self.add_to_owner(receiver, id);
        token.owner = FixedAccount::of(receiver);
        token.agent_wallet = FixedAccount::NONE;
        token.pending_wallet = FixedAccount::NONE;
        token.pending_deadline = 0;
        self.tokens.insert(id, token);
        // Como `_update` de EVM: primero se limpia agentWallet, despues se mueve el token.
        emit_agent_wallet(id, None);
        emit_nep171(
            "nft_transfer",
            &NftTransferData {
                authorized_id: (sender != &owner).then(|| sender.clone()),
                old_owner_id: owner.clone(),
                new_owner_id: receiver.clone(),
                token_ids: vec![id.to_string()],
                memo,
            },
        );
        Transferred {
            previous_owner: owner,
            approvals,
            freed_bytes,
        }
    }
}

#[near]
impl IdentityRegistry {
    /// NEP-171. Exactamente 1 yocto. El dueno o un aprobado (si pasa `approval_id`, tiene que ser el suyo).
    #[payable]
    pub fn nft_transfer(
        &mut self,
        receiver_id: AccountId,
        token_id: String,
        approval_id: Option<u64>,
        memo: Option<String>,
    ) {
        assert_one_yocto();
        let sender = env::predecessor_account_id();
        let t = self.internal_transfer(&sender, &receiver_id, &token_id, approval_id, memo);
        refund(&t.previous_owner, bytes_cost(t.freed_bytes));
    }

    /// NEP-171. Exactamente 1 yocto y al menos `MIN_GAS_NFT_TRANSFER_CALL`. Si `receiver_id.nft_on_transfer` devuelve
    /// `true` (o falla), `nft_resolve_transfer` devuelve el agente; `agentWallet` queda limpia igual (desviacion I-5).
    #[payable]
    pub fn nft_transfer_call(
        &mut self,
        receiver_id: AccountId,
        token_id: String,
        approval_id: Option<u64>,
        memo: Option<String>,
        msg: String,
    ) -> PromiseOrValue<bool> {
        assert_one_yocto();
        if env::prepaid_gas() < MIN_GAS_NFT_TRANSFER_CALL {
            IdentityError::E21GasInsuficiente.panic();
        }
        let sender = env::predecessor_account_id();
        let t = self.internal_transfer(&sender, &receiver_id, &token_id, approval_id, memo);
        let canonical_id = parse_id(&token_id).to_string();
        ext_nft_receiver::ext(receiver_id.clone())
            .with_static_gas(GAS_FOR_NFT_ON_TRANSFER)
            .nft_on_transfer(sender, t.previous_owner.clone(), canonical_id.clone(), msg)
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_RESOLVE_TRANSFER)
                    .with_unused_gas_weight(0)
                    .nft_resolve_transfer(
                        t.previous_owner,
                        receiver_id,
                        canonical_id,
                        t.approvals,
                        U128(bytes_cost(t.freed_bytes)),
                    ),
            )
            .into()
    }

    /// Callback de `nft_transfer_call` (NEP-171). Devuelve `true` si la transferencia queda, `false` si se deshizo.
    /// Si queda, el storage de las aprobaciones borradas vuelve al dueno anterior; si se deshace, vuelven sus
    /// aprobaciones y las que hubiera puesto el receptor se borran y su storage vuelve al receptor.
    #[private]
    pub fn nft_resolve_transfer(
        &mut self,
        previous_owner_id: AccountId,
        receiver_id: AccountId,
        token_id: String,
        approved_account_ids: Option<BTreeMap<AccountId, u64>>,
        approvals_refund: U128,
    ) -> bool {
        let must_revert = match env::promise_result_checked(0, 16) {
            Ok(v) => serde_json::from_slice::<bool>(&v).unwrap_or(true),
            Err(_) => true,
        };
        let id = parse_id(&token_id);
        let mut token = self
            .tokens
            .get(&id)
            .cloned()
            .unwrap_or_else(|| env::abort());
        if !must_revert || !token.owner.is(&receiver_id) {
            refund(&previous_owner_id, approvals_refund.0);
            return true;
        }
        let before = self.usage();
        self.approvals.remove(&id);
        let freed = before.saturating_sub(self.usage());
        refund(&receiver_id, bytes_cost(freed));
        if let Some(a) = approved_account_ids.filter(|a| !a.is_empty()) {
            self.approvals.insert(id, a);
        }
        self.remove_from_owner(&receiver_id, token.owner_pos);
        token.owner_pos = self.add_to_owner(&previous_owner_id, id);
        token.owner = FixedAccount::of(&previous_owner_id);
        token.agent_wallet = FixedAccount::NONE;
        token.pending_wallet = FixedAccount::NONE;
        token.pending_deadline = 0;
        self.tokens.insert(id, token);
        emit_agent_wallet(id, None);
        emit_nep171(
            "nft_transfer",
            &NftTransferData {
                authorized_id: None,
                old_owner_id: receiver_id,
                new_owner_id: previous_owner_id,
                token_ids: vec![id.to_string()],
                memo: None,
            },
        );
        false
    }

    /// NEP-178: solo el dueno. Al menos 1 yocto y lo que crece (vuelve el sobrante). Con `msg`, llama
    /// `account_id.nft_on_approve`. Hasta `MAX_APPROVALS` cuentas por agente.
    #[payable]
    pub fn nft_approve(
        &mut self,
        token_id: String,
        account_id: AccountId,
        msg: Option<String>,
    ) -> Option<Promise> {
        let deposit = require_deposit();
        let caller = env::predecessor_account_id();
        let (id, mut token) = self.token_or_panic(&token_id);
        if !token.owner.is(&caller) {
            IdentityError::E22SoloElDueno.panic();
        }
        let mut map = self.approvals.get(&id).cloned().unwrap_or_default();
        if !map.contains_key(&account_id) && map.len() >= MAX_APPROVALS {
            IdentityError::E23DemasiadasAprobaciones.panic();
        }
        let approval_id = token.next_approval_id;
        token.next_approval_id = approval_id.checked_add(1).unwrap_or_else(|| env::abort());
        let before = self.usage();
        map.insert(account_id.clone(), approval_id);
        self.approvals.insert(id, map);
        self.tokens.insert(id, token);
        let after = self.usage();
        self.settle(before, after, 0, &caller, &caller, deposit);
        msg.map(|msg| {
            ext_nft_approval_receiver::ext(account_id)
                .with_static_gas(GAS_FOR_NFT_ON_APPROVE)
                .nft_on_approve(id.to_string(), caller, approval_id, msg)
        })
    }

    /// NEP-178: solo el dueno, exactamente 1 yocto. El storage liberado vuelve al dueno.
    #[payable]
    pub fn nft_revoke(&mut self, token_id: String, account_id: AccountId) {
        assert_one_yocto();
        let caller = env::predecessor_account_id();
        let (id, token) = self.token_or_panic(&token_id);
        if !token.owner.is(&caller) {
            IdentityError::E22SoloElDueno.panic();
        }
        let Some(mut map) = self.approvals.get(&id).cloned() else {
            return;
        };
        if map.remove(&account_id).is_none() {
            return;
        }
        let before = self.usage();
        if map.is_empty() {
            self.approvals.remove(&id);
        } else {
            self.approvals.insert(id, map);
        }
        let freed = before.saturating_sub(self.usage());
        refund(&caller, bytes_cost(freed));
    }

    /// NEP-178: solo el dueno, exactamente 1 yocto.
    #[payable]
    pub fn nft_revoke_all(&mut self, token_id: String) {
        assert_one_yocto();
        let caller = env::predecessor_account_id();
        let (id, token) = self.token_or_panic(&token_id);
        if !token.owner.is(&caller) {
            IdentityError::E22SoloElDueno.panic();
        }
        let before = self.usage();
        self.approvals.remove(&id);
        let freed = before.saturating_sub(self.usage());
        refund(&caller, bytes_cost(freed));
    }
}
