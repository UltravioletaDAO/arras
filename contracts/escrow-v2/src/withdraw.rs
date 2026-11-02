//! Retiros sin candado y `compact` (sec. 3.3, 3.5 y 3.6), como v1. `compact` de v2 acepta `Resolved` y cuenta la
//! retencion desde la resolucion si fue despues de `reclaim_after`.
//!
//! `withdraw`/`withdraw_for` ponen `claimable` en 0 **sin borrar la entrada**, suman a `in_flight` y hacen
//! `ft_transfer` (1 yocto, 10 TGas) con `on_withdraw` (10 TGas). `on_withdraw` Ok baja `tvl_global` (la unica vez
//! que baja); `Err` suma de vuelta (`+=`, asi un `release` intercalado no se pierde) y no necesita bytes nuevos
//! porque la entrada existe en 0. Dos retiros en vuelo son seguros sin candado: el segundo retira 0 (`E30`) o lo
//! nuevo.

use crate::*;
use arras_escrow_core::decode_payment_id;
use arras_escrow_core::events::{ArrasEvent, CompactedData, WithdrawnData};
use near_contract_standards::fungible_token::core::ext_ft_core;
use near_sdk::{Promise, PromiseError};

#[near]
impl Arras {
    /// El titular retira todo su `claimable`. Funciona en pausa.
    pub fn withdraw(&mut self) -> Promise {
        self.start_withdraw(env::predecessor_account_id())
    }

    /// Cualquiera dispara el retiro de `account` (la plata va a `account`). Funciona en pausa.
    pub fn withdraw_for(&mut self, account: AccountId) -> Promise {
        self.start_withdraw(account)
    }

    /// Callback de cada retiro. `Failed` -> `claimable += amount`, `in_flight -= amount`, `withdrawn ok:false` y
    /// devuelve 0; exito -> `tvl_global -= amount` y devuelve lo transferido.
    ///
    /// No usa `#[callback_result] r: Result<(), PromiseError>` (sec. 3.3): esa macro deserializa lo que devuelve el
    /// token y entra en panico si no es vacio ni `null`, y un panico aca, despues de un `ft_transfer` exitoso, dejaria
    /// `in_flight` y `tvl_global` arriba para siempre (el USDC de Circle es actualizable). Se lee el resultado sin
    /// mirar los bytes: solo `Failed` es falla; un exito con datos (`TooLong` con `max_len` 0) cuenta como Ok, asi un
    /// transfer hecho nunca se acredita dos veces. Refutacion de H-1, P3-2.
    #[private]
    pub fn on_withdraw(&mut self, account: AccountId, amount: U128) -> U128 {
        let ok = !matches!(env::promise_result_checked(0, 0), Err(PromiseError::Failed));
        self.in_flight = self
            .in_flight
            .checked_sub(amount.0)
            .unwrap_or_else(|| env::abort());
        if ok {
            self.tvl_global = self
                .tvl_global
                .checked_sub(amount.0)
                .unwrap_or_else(|| env::abort());
        } else {
            self.credit(&account, amount.0);
        }
        ArrasEvent::Withdrawn(vec![WithdrawnData {
            account,
            token: self.token.clone(),
            amount,
            ok,
        }])
        .emit();
        if ok { amount } else { U128(0) }
    }

    /// Cualquiera, con el pago en estado terminal (`Released`, `Refunded`, `Reclaimed` o `Resolved`) y `now >=
    /// max(reclaim_after, resolved_at) + 7 d`. Borra el `Payment`, guarda la lapida en `used_ids` y devuelve a quien
    /// lo pago lo que el borrado libera de verdad (medido con `env::storage_usage()`, menos la lapida) mas, si nunca
    /// hubo disputa, la reserva de la disputa que no se uso; con tope en `storage_charged`.
    pub fn compact(&mut self, payment_id: PaymentId) {
        let p = self.payment_or_e20(&payment_id);
        if !p.status.is_terminal() {
            ArrasError::E22Estado("compact").panic();
        }
        let resuelto = p
            .dispute
            .as_ref()
            .and_then(|d| d.resolved_at)
            .map_or(0, |t| t.0);
        let desde = p.reclaim_after.0.max(resuelto);
        if env::block_timestamp() < desde.saturating_add(RETENTION_NS) {
            ArrasError::E28Retencion.panic();
        }
        let hash = decode_payment_id(&payment_id).unwrap_or_else(|| env::abort());
        self.payments.flush();
        let antes = env::storage_usage();
        self.payments.remove(&payment_id);
        self.payments.flush();
        self.used_ids.insert(hash);
        let mut liberado = antes.saturating_sub(env::storage_usage());
        if p.dispute.is_none() {
            liberado += DISPUTE_RESERVE_BYTES;
        }
        let devuelto = bytes_cost(liberado).min(p.storage_charged.0);
        self.refund_storage(&p.operator_id, &p.payer, p.sponsored, devuelto);
        ArrasEvent::Compacted(vec![CompactedData { payment_id }]).emit();
    }
}

impl Arras {
    fn start_withdraw(&mut self, account: AccountId) -> Promise {
        let amount = self.claimable_of(&account);
        if amount == 0 {
            ArrasError::E30SinSaldo.panic();
        }
        self.claimable.insert(account.clone(), 0);
        self.in_flight = self
            .in_flight
            .checked_add(amount)
            .unwrap_or_else(|| env::abort());
        ext_ft_core::ext(self.token.clone())
            .with_attached_deposit(NearToken::from_yoctonear(1))
            .with_static_gas(GAS_FT_TRANSFER)
            .ft_transfer(account.clone(), U128(amount), None)
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(GAS_ON_WITHDRAW)
                    .on_withdraw(account, U128(amount)),
            )
    }
}
