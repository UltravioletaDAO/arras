//! Administracion (sec. 3.3): `register_operator` y `fund_operator_storage` (C3); `update_operator`, `set_caps`,
//! `pause` y `unpause` (C7). Todo lo del owner lleva 1 yocto: sin yocto, panic de near-sdk.

use crate::*;
use arras_escrow_core::events::{ArrasEvent, OperatorData};
use arras_escrow_core::is_valid_operator_id;
use near_sdk::assert_one_yocto;

#[near]
impl Arras {
    /// Solo el owner, 1 yocto. `E40` -> `E45` (id fuera de `[a-z0-9_-]{1,32}`) -> `E44` -> `E43`.
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
}
