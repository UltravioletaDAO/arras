//! Administracion (sec. 3.3): `register_operator` y `fund_operator_storage` (C3); `update_operator`, `set_caps`,
//! `pause` y `unpause` (C7). Todo lo del owner lleva 1 yocto: sin yocto, panic de near-sdk.

use crate::*;
use arras_escrow_core::events::{ArrasEvent, CapsSetData, OperatorData, PauseData};
use arras_escrow_core::is_valid_operator_id;
use near_sdk::assert_one_yocto;

#[near]
impl Arras {
    /// Solo el owner, 1 yocto. `E40` -> `E45` (id fuera de `[a-z0-9_-]{1,32}`) -> `E44` -> `E43` -> `E46`
    /// (`fee_recipient` = el propio contrato).
    #[payable]
    pub fn register_operator(&mut self, operator_id: OperatorId, input: OperatorInput) {
        assert_one_yocto();
        self.assert_owner();
        if !is_valid_operator_id(&operator_id) {
            ArrasError::E45OperatorIdInvalido.panic();
        }
        if self.operators.contains_key(&operator_id) {
            ArrasError::E44OperadorExiste.panic();
        }
        if input.fee_bps > MAX_FEE_BPS {
            ArrasError::E43FeeBps.panic();
        }
        if input.fee_recipient == env::current_account_id() {
            ArrasError::E46FeeRecipientInvalido.panic();
        }
        let operator = Operator {
            owner: input.owner,
            capturers: input.capturers,
            fee_bps: input.fee_bps,
            fee_recipient: input.fee_recipient,
            min_amount: input.min_amount,
            tope_tvl_operador: input.tope_tvl_operador,
            tvl_operador: U128(0),
            tope_tvl_por_payer: input.tope_tvl_por_payer,
            sponsor_pool: U128(0),
            sponsor_max_por_payer: input.sponsor_max_por_payer,
        };
        ArrasEvent::OperatorRegistered(vec![OperatorData {
            operator_id: operator_id.clone(),
            owner: Some(operator.owner.clone()),
            capturers: Some(operator.capturers.clone()),
            fee_bps: Some(operator.fee_bps),
            fee_recipient: Some(operator.fee_recipient.clone()),
            min_amount: Some(operator.min_amount),
            tope_tvl_operador: Some(operator.tope_tvl_operador),
            tope_tvl_por_payer: Some(operator.tope_tvl_por_payer),
            sponsor_max_por_payer: Some(operator.sponsor_max_por_payer),
        }])
        .emit();
        self.payments_by_operator
            .insert(operator_id.clone(), Vector::new(index_prefix(&operator_id)));
        self.operators.insert(operator_id, operator);
    }

    /// Cualquiera; `#[payable]` **sin** `assert_one_yocto`: suma lo adjunto al `sponsor_pool` del operador.
    #[payable]
    pub fn fund_operator_storage(&mut self, operator_id: OperatorId) {
        let amount = env::attached_deposit().as_yoctonear();
        let Some(operator) = self.operators.get_mut(&operator_id) else {
            ArrasError::E04OperadorInexistente.panic();
        };
        operator.sponsor_pool = U128(
            operator
                .sponsor_pool
                .0
                .checked_add(amount)
                .unwrap_or_else(|| env::abort()),
        );
    }

    /// Solo el owner, 1 yocto. `E40` -> `E04` -> `E43` (fee > 1800) -> `E42` (un tope que sube) -> `E46`
    /// (`fee_recipient` = el propio contrato). No toca la
    /// `fee_bps` ni el `fee_recipient` de los pagos vivos (estan congelados en cada `Payment`); los `capturers`
    /// nuevos si valen para los pagos vivos (sec. 16, P3-9).
    #[payable]
    pub fn update_operator(&mut self, operator_id: OperatorId, patch: OperatorPatch) {
        assert_one_yocto();
        self.assert_owner();
        let Some(op) = self.operators.get_mut(&operator_id) else {
            ArrasError::E04OperadorInexistente.panic();
        };
        if patch.fee_bps.is_some_and(|f| f > MAX_FEE_BPS) {
            ArrasError::E43FeeBps.panic();
        }
        if patch
            .tope_tvl_operador
            .is_some_and(|t| t.0 > op.tope_tvl_operador.0)
            || patch
                .tope_tvl_por_payer
                .is_some_and(|t| t.0 > op.tope_tvl_por_payer.0)
        {
            ArrasError::E42SoloBajarTopes.panic();
        }
        if patch.fee_recipient.as_ref() == Some(&env::current_account_id()) {
            ArrasError::E46FeeRecipientInvalido.panic();
        }
        if let Some(v) = &patch.capturers {
            op.capturers = v.clone();
        }
        if let Some(v) = patch.fee_bps {
            op.fee_bps = v;
        }
        if let Some(v) = &patch.fee_recipient {
            op.fee_recipient = v.clone();
        }
        if let Some(v) = patch.min_amount {
            op.min_amount = v;
        }
        if let Some(v) = patch.tope_tvl_operador {
            op.tope_tvl_operador = v;
        }
        if let Some(v) = patch.tope_tvl_por_payer {
            op.tope_tvl_por_payer = v;
        }
        if let Some(v) = patch.sponsor_max_por_payer {
            op.sponsor_max_por_payer = v;
        }
        ArrasEvent::OperatorUpdated(vec![OperatorData {
            operator_id,
            owner: None,
            capturers: patch.capturers,
            fee_bps: patch.fee_bps,
            fee_recipient: patch.fee_recipient,
            min_amount: patch.min_amount,
            tope_tvl_operador: patch.tope_tvl_operador,
            tope_tvl_por_payer: patch.tope_tvl_por_payer,
            sponsor_max_por_payer: patch.sponsor_max_por_payer,
        }])
        .emit();
    }

    /// Solo el owner, 1 yocto; los topes globales solo bajan (`E42`). `new` es el techo para siempre.
    #[payable]
    pub fn set_caps(&mut self, tope_por_pago: Option<U128>, tope_tvl_global: Option<U128>) {
        assert_one_yocto();
        self.assert_owner();
        if tope_por_pago.is_some_and(|t| t.0 > self.tope_por_pago)
            || tope_tvl_global.is_some_and(|t| t.0 > self.tope_tvl_global)
        {
            ArrasError::E42SoloBajarTopes.panic();
        }
        if let Some(t) = tope_por_pago {
            self.tope_por_pago = t.0;
        }
        if let Some(t) = tope_tvl_global {
            self.tope_tvl_global = t.0;
        }
        ArrasEvent::CapsSet(vec![CapsSetData {
            by: env::predecessor_account_id(),
            tope_por_pago: U128(self.tope_por_pago),
            tope_tvl_global: U128(self.tope_tvl_global),
        }])
        .emit();
    }

    /// El guardian o el owner, 1 yocto (`E41`). Bloquea authorize (`E02`); `release`, `refund`, `reclaim`,
    /// `withdraw*` y `compact` siguen.
    #[payable]
    pub fn pause(&mut self) {
        assert_one_yocto();
        let by = env::predecessor_account_id();
        if by != self.guardian && by != self.owner {
            ArrasError::E41SoloGuardianUOwner.panic();
        }
        self.paused = true;
        ArrasEvent::Paused(vec![PauseData {
            by,
            paused: true,
            at: U64(env::block_timestamp()),
        }])
        .emit();
    }

    /// Solo el owner, 1 yocto (`E40`).
    #[payable]
    pub fn unpause(&mut self) {
        assert_one_yocto();
        self.assert_owner();
        self.paused = false;
        ArrasEvent::Unpaused(vec![PauseData {
            by: env::predecessor_account_id(),
            paused: false,
            at: U64(env::block_timestamp()),
        }])
        .emit();
    }
}
