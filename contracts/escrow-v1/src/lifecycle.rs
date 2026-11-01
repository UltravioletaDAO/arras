//! Ciclo de vida sin promesas (sec. 3.3): `release`, `refund` y `reclaim` solo acreditan `claimable`. Los tres
//! funcionan en pausa (refutacion P3-18: un trabajo terminado no se vuelve reembolso por la pausa).
//!
//! Guardas (sec. 3.4): `release` E20 -> E21 -> E22 -> E23 -> E24; `refund` E20 -> E25 -> E22; `reclaim` E20 ->
//! E26 -> E27 -> E22. TVL (sec. 3.5): lo que sale de "vivo" baja `tvl_operador` y `payer_tvl`; `tvl_global` no
//! baja aca (baja solo cuando la plata sale del contrato, en `on_withdraw` Ok).

use crate::*;
use arras_escrow_core::events::{ArrasEvent, PaymentAmountData, ReleasedData};
use arras_escrow_core::fee_split;

#[near]
impl Arras {
    /// Solo un capturer del operador, antes de `release_deadline`. `amount = None` libera todo lo vivo. La tarifa es
    /// la congelada en el pago: `fee = floor(x * fee_bps / 10000)` al `fee_recipient` congelado, `net` al receiver.
    pub fn release(&mut self, payment_id: PaymentId, amount: Option<U128>) {
        let mut p = self.payment_or_e20(&payment_id);
        let caller = env::predecessor_account_id();
        if !self.is_capturer(&p.operator_id, &caller) {
            ArrasError::E21NoEsCapturer.panic();
        }
        if !p.status.is_live() {
            ArrasError::E22Estado("release").panic();
        }
        if env::block_timestamp() >= p.release_deadline.0 {
            ArrasError::E23PlazoVencido.panic();
        }
        let live = p.live_amount();
        let x = amount.map_or(live, |a| a.0);
        if x == 0 || x > live {
            ArrasError::E24Monto.panic();
        }
        let split = fee_split(x, p.fee_bps).unwrap_or_else(|| env::abort());
        p.released = U128(p.released.0.checked_add(x).unwrap_or_else(|| env::abort()));
        p.status = if x == live {
            PaymentStatus::Released
        } else {
            PaymentStatus::PartiallyReleased
        };
        self.credit(&p.receiver, split.net);
        self.credit(&p.fee_recipient, split.fee);
        self.leave_live(&p.operator_id, &p.payer, x);
        self.payments.insert(payment_id.clone(), p);
        ArrasEvent::Released(vec![ReleasedData {
            payment_id,
            amount: U128(x),
            net: U128(split.net),
            fee: U128(split.fee),
        }])
        .emit();
    }

    /// Un capturer o el receiver (renuncia). Lo no liberado va a `claimable[payer]`; lo liberado antes queda donde
    /// esta.
    pub fn refund(&mut self, payment_id: PaymentId) {
        let p = self.payment_or_e20(&payment_id);
        let caller = env::predecessor_account_id();
        if caller != p.receiver && !self.is_capturer(&p.operator_id, &caller) {
            ArrasError::E25NoAutorizado.panic();
        }
        if !p.status.is_live() {
            ArrasError::E22Estado("refund").panic();
        }
        let amount = self.close_to_payer(&payment_id, p, PaymentStatus::Refunded);
        ArrasEvent::Refunded(vec![PaymentAmountData {
            payment_id,
            amount: U128(amount),
        }])
        .emit();
    }

    /// Solo el payer, desde `reclaim_after` (inclusive).
    pub fn reclaim(&mut self, payment_id: PaymentId) {
        let p = self.payment_or_e20(&payment_id);
        if env::predecessor_account_id() != p.payer {
            ArrasError::E26NoEsPayer.panic();
        }
        if env::block_timestamp() < p.reclaim_after.0 {
            ArrasError::E27TodaviaNo.panic();
        }
        if !p.status.is_live() {
            ArrasError::E22Estado("reclaim").panic();
        }
        let amount = self.close_to_payer(&payment_id, p, PaymentStatus::Reclaimed);
        ArrasEvent::Reclaimed(vec![PaymentAmountData {
            payment_id,
            amount: U128(amount),
        }])
        .emit();
    }
}

impl Arras {
    pub(crate) fn payment_or_e20(&self, payment_id: &str) -> Payment {
        self.payments
            .get(payment_id)
            .cloned()
            .unwrap_or_else(|| ArrasError::E20PagoInexistente.panic())
    }

    /// Los capturers **actuales** del operador (`update_operator` los puede cambiar; sec. 16, P3-9).
    pub(crate) fn is_capturer(&self, operator_id: &str, account: &AccountId) -> bool {
        self.operators
            .get(operator_id)
            .is_some_and(|o| o.capturers.contains(account))
    }

    /// `claimable[account] += amount`. La entrada existe: authorize la precargo.
    pub(crate) fn credit(&mut self, account: &AccountId, amount: u128) {
        if amount == 0 {
            return;
        }
        let actual = self.claimable_of(account);
        self.claimable.insert(
            account.clone(),
            actual.checked_add(amount).unwrap_or_else(|| env::abort()),
        );
    }

    /// Lo que sale de "vivo": baja `tvl_operador` y `payer_tvl` (no `tvl_global`).
    fn leave_live(&mut self, operator_id: &str, payer: &AccountId, amount: u128) {
        let op = self
            .operators
            .get_mut(operator_id)
            .unwrap_or_else(|| env::abort());
        op.tvl_operador = U128(
            op.tvl_operador
                .0
                .checked_sub(amount)
                .unwrap_or_else(|| env::abort()),
        );
        let key = (operator_id.to_string(), payer.clone());
        let v = self.payer_tvl.get_mut(&key).unwrap_or_else(|| env::abort());
        *v = v.checked_sub(amount).unwrap_or_else(|| env::abort());
    }

    /// `refund` y `reclaim`: lo vivo va a `claimable[payer]` y el pago queda en `status`. Devuelve lo devuelto.
    fn close_to_payer(&mut self, payment_id: &str, mut p: Payment, status: PaymentStatus) -> u128 {
        let live = p.live_amount();
        p.refunded = U128(
            p.refunded
                .0
                .checked_add(live)
                .unwrap_or_else(|| env::abort()),
        );
        p.status = status;
        self.credit(&p.payer, live);
        self.leave_live(&p.operator_id, &p.payer, live);
        self.payments.insert(payment_id.to_string(), p);
        live
    }
}
