//! Retiros sin candado y `compact` (sec. 3.3, 3.5 y 3.6).
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

    /// Callback de cada retiro. `Err` -> `claimable += amount`, `in_flight -= amount`, `withdrawn ok:false` y
    /// devuelve 0; Ok -> `tvl_global -= amount` y devuelve lo transferido. `TooLong` no puede pasar (`max_bytes`
    /// es `usize::MAX`) y, si pasara, el transfer igual se hizo: cuenta como Ok, nunca se acredita dos veces.
    #[private]
    pub fn on_withdraw(
        &mut self,
        account: AccountId,
        amount: U128,
        #[callback_result] r: Result<(), PromiseError>,
    ) -> U128 {
        self.in_flight = self
            .in_flight
            .checked_sub(amount.0)
            .unwrap_or_else(|| env::abort());
        let ok = !matches!(r, Err(PromiseError::Failed));
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

    /// Cualquiera, con el pago en estado terminal y `now >= reclaim_after + 7 d`. Borra el `Payment`, guarda la
    /// lapida en `used_ids` y devuelve lo que el borrado libera de verdad (medido con `env::storage_usage()`, menos
    /// la lapida, con tope en `storage_charged`) a quien lo pago: el saldo NEP-145 del payer o el `sponsor_pool`.
    pub fn compact(&mut self, payment_id: PaymentId) {
        let p = self.payment_or_e20(&payment_id);
        if p.status.is_live() {
            ArrasError::E22Estado("compact").panic();
        }
        if env::block_timestamp() < p.reclaim_after.0.saturating_add(RETENTION_NS) {
            ArrasError::E28Retencion.panic();
        }
        let hash = decode_payment_id(&payment_id).unwrap_or_else(|| env::abort());
        self.payments.flush();
        let antes = env::storage_usage();
        self.payments.remove(&payment_id);
        self.payments.flush();
        self.used_ids.insert(hash);
        let liberado = bytes_cost(antes.saturating_sub(env::storage_usage()));
        let devuelto = liberado.min(p.storage_charged.0);
        if devuelto > 0 {
            if p.sponsored {
                let op = self
                    .operators
                    .get_mut(&p.operator_id)
                    .unwrap_or_else(|| env::abort());
                op.sponsor_pool = U128(
                    op.sponsor_pool
                        .0
                        .checked_add(devuelto)
                        .unwrap_or_else(|| env::abort()),
                );
                let key = (p.operator_id.clone(), p.payer.clone());
                if let Some(used) = self.sponsored_used.get_mut(&key) {
                    *used = used.saturating_sub(devuelto);
                }
            } else if let Some(e) = self.storage.get_mut(&p.payer) {
                e.available = e
                    .available
                    .checked_add(devuelto)
                    .unwrap_or_else(|| env::abort());
                self.storage_available_total = self
                    .storage_available_total
                    .checked_add(devuelto)
                    .unwrap_or_else(|| env::abort());
            }
        }
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
