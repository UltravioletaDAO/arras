//! Vistas (sec. 3.3): `get_payment`, `get_claimable`, `get_operator`, `get_config`, `get_storage_health`,
//! `list_payments` y `compute_payment_id`.

use crate::*;
// Con alias: `#[near]` exporta en la wasm un simbolo `compute_payment_id` (la vista) en este modulo.
use arras_escrow_core::{
    PaymentIdInput, compute_payment_id as core_payment_id, parse_authorize_msg,
};

#[near(serializers = [json])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigView {
    /// `"arras-v1"`.
    pub version: String,
    pub owner: AccountId,
    pub guardian: AccountId,
    pub paused: bool,
    pub token: AccountId,
    pub tope_por_pago: U128,
    pub tope_tvl_global: U128,
    pub tvl_global: U128,
    pub in_flight: U128,
    pub payments: u32,
    pub operators: u32,
}

/// Reserva libre = saldo - storage ocupado - `available` de NEP-145 - `sponsor_pool` de los operadores.
#[near(serializers = [json])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StorageHealth {
    pub balance: U128,
    pub storage_usage: U64,
    pub storage_cost: U128,
    pub nep145_available: U128,
    pub sponsor_pools: U128,
    pub free_reserve: U128,
    /// 0,2 NEAR.
    pub min_free_reserve: U128,
    /// `false` es la alarma.
    pub ok: bool,
}

#[near]
impl Arras {
    pub fn get_payment(&self, payment_id: PaymentId) -> Option<Payment> {
        self.payments.get(&payment_id).cloned()
    }

    pub fn get_claimable(&self, account: AccountId) -> U128 {
        U128(self.claimable_of(&account))
    }

    pub fn get_operator(&self, operator_id: OperatorId) -> Option<Operator> {
        self.operators.get(&operator_id).cloned()
    }

    pub fn get_config(&self) -> ConfigView {
        ConfigView {
            version: VERSION.to_string(),
            owner: self.owner.clone(),
            guardian: self.guardian.clone(),
            paused: self.paused,
            token: self.token.clone(),
            tope_por_pago: U128(self.tope_por_pago),
            tope_tvl_global: U128(self.tope_tvl_global),
            tvl_global: U128(self.tvl_global),
            in_flight: U128(self.in_flight),
            payments: self.payments.len(),
            operators: self.operators.len(),
        }
    }

    pub fn get_storage_health(&self) -> StorageHealth {
        let balance = env::account_balance().as_yoctonear();
        let storage_usage = env::storage_usage();
        let storage_cost = bytes_cost(storage_usage);
        let sponsor_pools = self
            .operators
            .values()
            .fold(0u128, |acc, o| acc.saturating_add(o.sponsor_pool.0));
        let free_reserve = balance
            .saturating_sub(storage_cost)
            .saturating_sub(self.storage_available_total)
            .saturating_sub(sponsor_pools);
        StorageHealth {
            balance: U128(balance),
            storage_usage: U64(storage_usage),
            storage_cost: U128(storage_cost),
            nep145_available: U128(self.storage_available_total),
            sponsor_pools: U128(sponsor_pools),
            free_reserve: U128(free_reserve),
            min_free_reserve: U128(MIN_FREE_RESERVE.as_yoctonear()),
            ok: free_reserve >= MIN_FREE_RESERVE.as_yoctonear(),
        }
    }

    /// Pagina el indice del operador; `limit` se recorta a 50 y los ids compactados se saltan.
    pub fn list_payments(
        &self,
        operator_id: OperatorId,
        from_index: u32,
        limit: u32,
    ) -> Vec<(PaymentId, Payment)> {
        let Some(index) = self.payments_by_operator.get(&operator_id) else {
            return vec![];
        };
        let end = from_index
            .saturating_add(limit.min(MAX_LIST_LIMIT))
            .min(index.len());
        (from_index..end)
            .filter_map(|i| index.get(i))
            .filter_map(|id| self.payments.get(id).map(|p| (id.clone(), p.clone())))
            .collect()
    }

    /// El `payment_id` que daria `ft_on_transfer(sender_id, amount, msg)` con el token de este contrato (para el
    /// SDK). Un `msg` invalido es `ARRAS_E03`.
    pub fn compute_payment_id(&self, sender_id: AccountId, amount: U128, msg: String) -> PaymentId {
        let parsed = parse_authorize_msg(&msg).unwrap_or_else(|e| e.panic());
        core_payment_id(&PaymentIdInput::from_msg(
            &parsed,
            self.token.as_str(),
            sender_id.as_str(),
            amount.0,
        ))
        .1
    }
}
