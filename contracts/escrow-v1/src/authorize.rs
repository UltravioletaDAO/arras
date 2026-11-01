//! `ft_on_transfer` (sec. 3.3 y 3.4): authorize. **Nunca entra en panico**: un rechazo es `Value(amount)` + el log
//! `arras:rechazo:<CODIGO>` y el estado queda intacto, porque todas las guardas corren sobre `&self`
//! ([`Arras::plan_authorize`]) antes de la primera escritura ([`Arras::apply_authorize`]).
//!
//! Guardas en orden (la primera que falla decide): E01 -> E02 -> E03 -> E04 -> E15 -> E05 -> E06 -> E08 -> E09 ->
//! E10 -> E11 -> E12 -> E13 -> E14.
//!
//! Storage (sec. 3.6): el costo exacto de lo que el pago escribe (el `Payment`, su entrada del indice, `payer_tvl`
//! y las entradas nuevas de `claimable` del receiver, el `fee_recipient` y el payer) se calcula antes de escribir y
//! lo paga el saldo NEP-145 del payer; si no alcanza, el `sponsor_pool` del operador hasta `sponsor_max_por_payer`
//! (sumando la entrada de `sponsored_used`). Si ninguno alcanza, `E14`.

use crate::*;
use arras_escrow_core::events::{ArrasEvent, AuthorizedData};
use arras_escrow_core::{
    PaymentIdInput, compute_payment_id as core_payment_id, parse_authorize_msg,
};
use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;
use near_sdk::{PromiseOrValue, borsh};

/// Todo lo que authorize escribe, calculado y validado antes de escribir.
pub(crate) struct AuthorizePlan {
    pub payment_id: PaymentId,
    pub payment: Payment,
    pub tvl_key: (OperatorId, AccountId),
    pub new_tvl_global: u128,
    pub new_tvl_operador: u128,
    pub new_payer_tvl: u128,
    pub new_claimable: Vec<AccountId>,
    /// `Some(nuevo_sponsored_used)` si paga el patrocinio.
    pub sponsor: Option<u128>,
}

/// Bytes de lo que escribe un pago, sin la entrada de `sponsored_used`.
pub(crate) struct AuthorizeBytes {
    pub base: u64,
    /// La entrada de `sponsored_used` si el patrocinio paga y todavia no existe.
    pub sponsor_extra: u64,
}

fn rechazo_si(cond: bool, e: ArrasError) -> Result<(), ArrasError> {
    if cond { Err(e) } else { Ok(()) }
}

/// `actual + amount <= tope`, sin desbordar (desborde = supera el tope).
fn cabe(actual: u128, amount: u128, tope: u128, e: ArrasError) -> Result<u128, ArrasError> {
    match actual.checked_add(amount) {
        Some(n) if n <= tope => Ok(n),
        _ => Err(e),
    }
}

#[near]
impl FungibleTokenReceiver for Arras {
    /// Solo `token`. `Value(0)` si autoriza; `Value(amount)` y `arras:rechazo:<CODIGO>` si no.
    fn ft_on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        match self.plan_authorize(&sender_id, amount.0, &msg) {
            Ok(plan) => {
                self.apply_authorize(plan);
                PromiseOrValue::Value(U128(0))
            }
            Err(e) => {
                env::log_str(&e.rejection_log());
                PromiseOrValue::Value(amount)
            }
        }
    }
}

impl Arras {
    /// Las guardas de la sec. 3.4, en orden, sin escribir nada.
    pub(crate) fn plan_authorize(
        &self,
        payer: &AccountId,
        amount: u128,
        msg: &str,
    ) -> Result<AuthorizePlan, ArrasError> {
        rechazo_si(
            env::predecessor_account_id() != self.token,
            ArrasError::E01TokenNoPermitido,
        )?;
        rechazo_si(self.paused, ArrasError::E02Pausado)?;
        let msg = parse_authorize_msg(msg)?;
        let operator = self
            .operators
            .get(&msg.operator_id)
            .ok_or(ArrasError::E04OperadorInexistente)?;
        rechazo_si(
            msg.receiver == env::current_account_id() || &msg.receiver == payer,
            ArrasError::E15ReceiverInvalido,
        )?;
        rechazo_si(
            amount < operator.min_amount.0,
            ArrasError::E05DebajoDelMinimo,
        )?;
        rechazo_si(amount > self.tope_por_pago, ArrasError::E06TopePorPago)?;
        let tvl_key = (msg.operator_id.clone(), payer.clone());
        let payer_tvl = self.payer_tvl.get(&tvl_key).copied().unwrap_or(0);
        let new_payer_tvl = cabe(
            payer_tvl,
            amount,
            operator.tope_tvl_por_payer.0,
            ArrasError::E08TvlPayer,
        )?;
        let new_tvl_operador = cabe(
            operator.tvl_operador.0,
            amount,
            operator.tope_tvl_operador.0,
            ArrasError::E09TvlOperador,
        )?;
        let new_tvl_global = cabe(
            self.tvl_global,
            amount,
            self.tope_tvl_global,
            ArrasError::E10TvlGlobal,
        )?;
        let now = env::block_timestamp();
        rechazo_si(
            !(now < msg.release_deadline && msg.release_deadline < msg.reclaim_after),
            ArrasError::E11Plazos,
        )?;
        rechazo_si(
            operator.fee_bps > msg.fee_bps_max,
            ArrasError::E12FeeMayorAlMaximo,
        )?;
        let (hash, payment_id) = core_payment_id(&PaymentIdInput::from_msg(
            &msg,
            self.token.as_str(),
            payer.as_str(),
            amount,
        ));
        rechazo_si(
            self.payments.contains_key(&payment_id) || self.used_ids.contains(&hash),
            ArrasError::E13IdUsado,
        )?;

        let mut payment = Payment {
            operator_id: msg.operator_id.clone(),
            payer: payer.clone(),
            receiver: msg.receiver.clone(),
            amount: U128(amount),
            released: U128(0),
            refunded: U128(0),
            fee_bps: operator.fee_bps,
            fee_recipient: operator.fee_recipient.clone(),
            fee_bps_max: msg.fee_bps_max,
            created_at: U64(now),
            release_deadline: U64(msg.release_deadline),
            reclaim_after: U64(msg.reclaim_after),
            terms_hash: msg.terms_hash_hex(),
            status: PaymentStatus::Authorized,
            storage_charged: U128(0),
            sponsored: false,
        };
        let mut new_claimable: Vec<AccountId> = Vec::with_capacity(3);
        for a in [&payment.receiver, &payment.fee_recipient, &payment.payer] {
            if !new_claimable.contains(a) && !self.claimable.contains_key(a) {
                new_claimable.push(a.clone());
            }
        }
        let bytes = self.authorize_bytes(&payment_id, &payment, &tvl_key, &new_claimable);

        // E14: primero el saldo NEP-145 del payer; si no alcanza, el patrocinio con su tope por payer.
        let cost_payer = bytes_cost(bytes.base);
        let payer_available = self.storage.get(payer).map_or(0, |e| e.available);
        let sponsor = if payer_available >= cost_payer {
            payment.storage_charged = U128(cost_payer);
            None
        } else {
            let cost = bytes_cost(bytes.base + bytes.sponsor_extra);
            let used = self.sponsored_used.get(&tvl_key).copied().unwrap_or(0);
            let new_used = cabe(
                used,
                cost,
                operator.sponsor_max_por_payer.0,
                ArrasError::E14SinStorage,
            )?;
            rechazo_si(operator.sponsor_pool.0 < cost, ArrasError::E14SinStorage)?;
            payment.storage_charged = U128(cost);
            payment.sponsored = true;
            Some(new_used)
        };

        Ok(AuthorizePlan {
            payment_id,
            payment,
            tvl_key,
            new_tvl_global,
            new_tvl_operador,
            new_payer_tvl,
            new_claimable,
            sponsor,
        })
    }

    /// Bytes exactos que escribe el pago (layout de `near_sdk::store` 5.29.1; los tests lo comparan con
    /// `env::storage_usage()`): `payments` (`pv`+u32 -> id y sha256(`pm`+id) -> Payment+u32), la entrada del
    /// indice, `payer_tvl` y `claimable` si son nuevas; aparte, `sponsored_used` si es nueva.
    pub(crate) fn authorize_bytes(
        &self,
        payment_id: &str,
        payment: &Payment,
        tvl_key: &(OperatorId, AccountId),
        new_claimable: &[AccountId],
    ) -> AuthorizeBytes {
        let id = payment_id.len() as u64;
        let payment_len = borsh::to_vec(payment)
            .map(|v| v.len())
            .unwrap_or_else(|_| env::abort()) as u64;
        let pair_key = 1 + 4 + tvl_key.0.len() as u64 + 4 + tvl_key.1.len() as u64;
        let mut base = record_bytes(2 + 4, 4 + id)
            + record_bytes(32, payment_len + 4)
            + record_bytes(index_prefix(&payment.operator_id).len() as u64 + 4, 4 + id);
        if !self.payer_tvl.contains_key(tvl_key) {
            base += record_bytes(pair_key, 16);
        }
        for a in new_claimable {
            base += record_bytes(1 + 4 + a.len() as u64, 16);
        }
        let sponsor_extra = if self.sponsored_used.contains_key(tvl_key) {
            0
        } else {
            record_bytes(pair_key, 16)
        };
        AuthorizeBytes {
            base,
            sponsor_extra,
        }
    }

    /// Escribe el plan. No falla: todo se valido en [`Self::plan_authorize`].
    pub(crate) fn apply_authorize(&mut self, plan: AuthorizePlan) {
        let AuthorizePlan {
            payment_id,
            payment,
            tvl_key,
            new_tvl_global,
            new_tvl_operador,
            new_payer_tvl,
            new_claimable,
            sponsor,
        } = plan;
        let cost = payment.storage_charged.0;
        let operator = self
            .operators
            .get_mut(&payment.operator_id)
            .unwrap_or_else(|| env::abort());
        operator.tvl_operador = U128(new_tvl_operador);
        let fee_recipient = operator.fee_recipient.clone();
        match sponsor {
            Some(new_used) => {
                operator.sponsor_pool = U128(operator.sponsor_pool.0 - cost);
                self.sponsored_used.insert(tvl_key.clone(), new_used);
            }
            None => {
                let entry = self
                    .storage
                    .get_mut(&payment.payer)
                    .unwrap_or_else(|| env::abort());
                entry.available -= cost;
                self.storage_available_total -= cost;
            }
        }
        self.tvl_global = new_tvl_global;
        self.payer_tvl.insert(tvl_key, new_payer_tvl);
        for a in new_claimable {
            self.claimable.insert(a, 0);
        }
        self.payments_by_operator
            .get_mut(&payment.operator_id)
            .unwrap_or_else(|| env::abort())
            .push(payment_id.clone());
        ArrasEvent::Authorized(vec![AuthorizedData {
            payment_id: payment_id.clone(),
            operator_id: payment.operator_id.clone(),
            token: self.token.clone(),
            payer: payment.payer.clone(),
            receiver: payment.receiver.clone(),
            amount: payment.amount,
            fee_bps: payment.fee_bps,
            fee_recipient,
            terms_hash: payment.terms_hash.clone(),
        }])
        .emit();
        self.payments.insert(payment_id, payment);
    }
}
