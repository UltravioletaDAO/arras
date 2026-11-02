//! Administracion (sec. 3.3), como v1, con lo que suma v2 al operador: `arbiter`, `arbiter_window_ns` (en
//! `[1 minuto, 30 dias]`, `E47`) y `ticket_pubkey` (solo ed25519, `E48`). Todo lo del owner lleva 1 yocto: sin yocto,
//! panic de near-sdk. La pausa bloquea authorize, los depositos de presupuesto y `authorize_from_budget`; release,
//! refund, reclaim, disputas, `withdraw*`, `withdraw_budget`, `revoke_agent` y `compact` siguen.

use crate::error::ArrasErrorV2;
use crate::events::{ArrasEventV2, OperatorDataV2};
use crate::*;
use arras_escrow_core::events::{ArrasEvent, CapsSetData, PauseData};
use arras_escrow_core::is_valid_operator_id;
use near_sdk::CurveType;
use near_sdk::assert_one_yocto;

fn check_window(w: U64) {
    if !(MIN_ARBITER_WINDOW_NS..=MAX_ARBITER_WINDOW_NS).contains(&w.0) {
        ArrasErrorV2::E47VentanaArbitro.panic();
    }
}

fn check_ticket_pubkey(pk: &PublicKey) {
    if pk.curve_type() != CurveType::ED25519 || pk.as_bytes().len() != 33 {
        ArrasErrorV2::E48TicketPubkey.panic();
    }
}

#[near]
impl Arras {
    /// Solo el owner, 1 yocto. `E40` -> `E45` (id fuera de `[a-z0-9_-]{1,32}`) -> `E44` -> `E43` -> `E46`
    /// (`fee_recipient` = el propio contrato) -> `E47` (plazo del arbitro) -> `E48` (llave del ticket).
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
        check_window(input.arbiter_window_ns);
        if let Some(pk) = &input.ticket_pubkey {
            check_ticket_pubkey(pk);
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
            arbiter: input.arbiter,
            arbiter_window_ns: input.arbiter_window_ns,
            ticket_pubkey: input.ticket_pubkey,
        };
        ArrasEventV2::OperatorRegistered(vec![OperatorDataV2 {
            operator_id: operator_id.clone(),
            owner: Some(operator.owner.clone()),
            capturers: Some(operator.capturers.clone()),
            fee_bps: Some(operator.fee_bps),
            fee_recipient: Some(operator.fee_recipient.clone()),
            min_amount: Some(operator.min_amount),
            tope_tvl_operador: Some(operator.tope_tvl_operador),
            tope_tvl_por_payer: Some(operator.tope_tvl_por_payer),
            sponsor_max_por_payer: Some(operator.sponsor_max_por_payer),
            arbiter: Some(operator.arbiter.clone()),
            arbiter_window_ns: Some(operator.arbiter_window_ns),
            ticket_pubkey: operator.ticket_pubkey.clone(),
            quitar_ticket_pubkey: None,
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
    /// (`fee_recipient` = el propio contrato) -> `E47` -> `E48` (llave no ed25519, o poner y quitar a la vez). No toca
    /// la `fee_bps`, el `fee_recipient` ni el `arbiter_window_ns` de los pagos vivos (congelados en cada `Payment`);
    /// los `capturers` y el `arbiter` nuevos si valen para los pagos vivos (rotar una llave comprometida, P3-9).
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
        if let Some(w) = patch.arbiter_window_ns {
            check_window(w);
        }
        let quitar = patch.quitar_ticket_pubkey == Some(true);
        if let Some(pk) = &patch.ticket_pubkey {
            if quitar {
                ArrasErrorV2::E48TicketPubkey.panic();
            }
            check_ticket_pubkey(pk);
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
        if let Some(v) = &patch.arbiter {
            op.arbiter = v.clone();
        }
        if let Some(v) = patch.arbiter_window_ns {
            op.arbiter_window_ns = v;
        }
        if let Some(v) = &patch.ticket_pubkey {
            op.ticket_pubkey = Some(v.clone());
        }
        if quitar {
            op.ticket_pubkey = None;
        }
        ArrasEventV2::OperatorUpdated(vec![OperatorDataV2 {
            operator_id,
            owner: None,
            capturers: patch.capturers,
            fee_bps: patch.fee_bps,
            fee_recipient: patch.fee_recipient,
            min_amount: patch.min_amount,
            tope_tvl_operador: patch.tope_tvl_operador,
            tope_tvl_por_payer: patch.tope_tvl_por_payer,
            sponsor_max_por_payer: patch.sponsor_max_por_payer,
            arbiter: patch.arbiter,
            arbiter_window_ns: patch.arbiter_window_ns,
            ticket_pubkey: patch.ticket_pubkey,
            quitar_ticket_pubkey: patch.quitar_ticket_pubkey.filter(|q| *q),
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

    /// El guardian o el owner, 1 yocto (`E41`). Bloquea authorize, los depositos de presupuesto y
    /// `authorize_from_budget` (`E02`); el resto sigue (ver el encabezado del modulo).
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
