//! Vistas: las de v1 (`get_payment`, `get_claimable`, `get_operator`, `get_config`, `get_storage_health`,
//! `list_payments`, `compute_payment_id`) y las de v2: `get_budget`, `compute_budget_payment_id` y los bytes que firma
//! el operador (`ticket_message_payment`, `ticket_message_budget`), para que el SDK y EM no los adivinen.

use crate::msg::{parse_authorize_msg_v2, parse_hex_16, parse_hex_32};
use crate::ticket::{TicketSubject, ticket_message};
use crate::*;
// Con alias: `#[near]` exporta en la wasm un simbolo `compute_payment_id` (la vista) en este modulo.
use arras_escrow_core::{AuthorizeMsg, PaymentIdInput, compute_payment_id as core_payment_id};
use near_sdk::json_types::Base64VecU8;

/// `get_budget`: el presupuesto y lo que el agente todavia puede autorizar hoy.
#[near(serializers = [json])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BudgetView {
    pub token: AccountId,
    pub saldo: U128,
    pub tope_por_pago: U128,
    pub tope_diario: U128,
    /// Lo gastado en el dia UTC de hoy (0 si el ultimo gasto fue otro dia).
    pub gastado_hoy: U128,
    /// `min(saldo, tope_diario - gastado_hoy)`.
    pub disponible_hoy: U128,
    pub storage_charged: U128,
    pub sponsored: bool,
}

#[near(serializers = [json])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigView {
    /// `"arras-v2"`.
    pub version: String,
    pub owner: AccountId,
    pub guardian: AccountId,
    pub paused: bool,
    pub token: AccountId,
    pub tope_por_pago: U128,
    pub tope_tvl_global: U128,
    pub tvl_global: U128,
    pub in_flight: U128,
    /// Suma de los saldos de los presupuestos (parte de `tvl_global`).
    pub budgets_total: U128,
    pub payments: u32,
    pub operators: u32,
}

/// Reserva libre = saldo - storage ocupado - `available` de NEP-145 - `sponsor_pool` de los operadores - la reserva de
/// disputa cobrada y sin usar.
#[near(serializers = [json])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StorageHealth {
    pub balance: U128,
    pub storage_usage: U64,
    pub storage_cost: U128,
    pub nep145_available: U128,
    pub sponsor_pools: U128,
    /// Reserva de disputa cobrada y sin usar: es de los pagos, no del contrato.
    pub dispute_reserve: U128,
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
            budgets_total: U128(self.budgets_total),
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
            .saturating_sub(sponsor_pools)
            .saturating_sub(self.dispute_reserve_total);
        StorageHealth {
            balance: U128(balance),
            storage_usage: U64(storage_usage),
            storage_cost: U128(storage_cost),
            nep145_available: U128(self.storage_available_total),
            sponsor_pools: U128(sponsor_pools),
            dispute_reserve: U128(self.dispute_reserve_total),
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

    /// El `payment_id` que daria `ft_on_transfer(sender_id, amount, msg)` con un `msg` `authorize` (de v1 o de v2) y el
    /// token de este contrato. Un `msg` invalido (o uno `budget`) es `ARRAS_E03`.
    pub fn compute_payment_id(&self, sender_id: AccountId, amount: U128, msg: String) -> PaymentId {
        let parsed = parse_authorize_msg_v2(&msg).unwrap_or_else(|e| e.panic());
        core_payment_id(&PaymentIdInput::from_msg(
            &parsed.base,
            self.token.as_str(),
            sender_id.as_str(),
            amount.0,
        ))
        .1
    }

    /// El `payment_id` que daria `authorize_from_budget` con estos argumentos: el de la sec. 7 con `payer = owner`.
    /// Hex invalido: `ARRAS_E03`.
    #[allow(clippy::too_many_arguments)]
    pub fn compute_budget_payment_id(
        &self,
        owner: AccountId,
        operator_id: OperatorId,
        receiver: AccountId,
        amount: U128,
        release_deadline: U64,
        reclaim_after: U64,
        fee_bps_max: u16,
        terms_hash: String,
        salt: String,
    ) -> PaymentId {
        let (Some(terms_hash), Some(salt)) = (parse_hex_32(&terms_hash), parse_hex_16(&salt))
        else {
            ArrasError::E03MsgInvalido.panic()
        };
        let m = AuthorizeMsg {
            operator_id,
            receiver,
            release_deadline: release_deadline.0,
            reclaim_after: reclaim_after.0,
            fee_bps_max,
            terms_hash,
            salt,
        };
        core_payment_id(&PaymentIdInput::from_msg(
            &m,
            self.token.as_str(),
            owner.as_str(),
            amount.0,
        ))
        .1
    }

    pub fn get_budget(
        &self,
        owner: AccountId,
        agent: AccountId,
        operator_id: OperatorId,
    ) -> Option<BudgetView> {
        let b = self.budgets.get(&(owner, agent, operator_id))?;
        let hoy = env::block_timestamp() / DAY_NS;
        let gastado = if b.dia.0 == hoy { b.gastado_hoy.0 } else { 0 };
        let disponible = b.saldo.0.min(b.tope_diario.0.saturating_sub(gastado));
        Some(BudgetView {
            token: self.token.clone(),
            saldo: b.saldo,
            tope_por_pago: b.tope_por_pago,
            tope_diario: b.tope_diario,
            gastado_hoy: U128(gastado),
            disponible_hoy: U128(disponible),
            storage_charged: b.storage_charged,
            sponsored: b.sponsored,
        })
    }

    /// Los bytes que el operador firma para el ticket de un pago (base64 en JSON).
    pub fn ticket_message_payment(&self, payment_id: PaymentId, expiry: U64) -> Base64VecU8 {
        Base64VecU8(ticket_message(
            env::current_account_id().as_str(),
            TicketSubject::Payment {
                payment_id: &payment_id,
            },
            expiry.0,
        ))
    }

    /// Los bytes que el operador firma para el ticket de un deposito de presupuesto (base64 en JSON).
    pub fn ticket_message_budget(
        &self,
        operator_id: OperatorId,
        owner: AccountId,
        agent: AccountId,
        expiry: U64,
    ) -> Base64VecU8 {
        Base64VecU8(ticket_message(
            env::current_account_id().as_str(),
            TicketSubject::Budget {
                operator_id: &operator_id,
                owner: owner.as_str(),
                agent: agent.as_str(),
            },
            expiry.0,
        ))
    }
}
