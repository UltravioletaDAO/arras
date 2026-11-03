//! Presupuesto de agente (prescripcion sec. 4; plan v2 I-3 y F-8, el camino de pago de EM en v2).
//!
//! El humano (owner) deposita con `ft_transfer_call` y `msg` `budget` (lo valida [`crate::authorize`]). El agente es
//! **siempre una cuenta** (en NEP-366 el `predecessor_account_id` del receipt interno es la cuenta que firmo el
//! delegate) y autoriza pagos con `authorize_from_budget`, que no es `#[payable]`: la firma una **function-call key
//! limitada a este contrato y a ese metodo, sin FullAccess** (una function-call key no puede adjuntar deposito). Lo
//! que el agente puede hacer con esa llave tiene tres limites que pone el contrato:
//!
//! 1. el tope por pago del presupuesto (`E52`) y el global del contrato (`E06`);
//! 2. el tope de la ventana diaria (dia UTC, `E51`) y el saldo (`E50`);
//! 3. el horizonte: `reclaim_after <= now + 60 dias` (`E57`), asi el owner siempre recupera lo que el agente autorizo
//!    y nadie libero;
//! 4. el pago queda en escrow: el agente no lo libera. Lo libera el capturer del operador al receiver, y un refund o
//!    un reclaim vuelve al **owner** (el payer del pago), nunca al agente ni al presupuesto.
//!
//! | Metodo | Quien | Guardas | En pausa |
//! |---|---|---|---|
//! | `authorize_from_budget(owner, operator_id, receiver, amount, release_deadline, reclaim_after, dispute_deadline?, fee_bps_max, terms_hash, salt, operator_ticket?)` | el agente | E02 -> E03 -> E04 -> E50 (no hay presupuesto) -> E15 (receiver = contrato, owner o agente) -> E24 (monto 0) -> E05 -> E06 -> E52 -> E51 -> E50 (saldo) -> E57 (`reclaim_after` a mas de 60 dias) -> E11 -> E12 -> E13 -> E53/E54 -> E14 | no |
//! | `withdraw_budget(agent, operator_id)` | el owner | E50 -> E30 | si |
//! | `revoke_agent(agent, operator_id)` | el owner | E50 | si |
//! | `set_budget_caps(agent, operator_id, tope_por_pago, tope_diario)` | el owner, 1 yocto | E50 -> E56 | si |
//!
//! `withdraw_budget` y `revoke_agent` llevan el saldo al `claimable` del owner (sale por `withdraw`, como todo). Un
//! re-deposito solo baja los topes; subirlos es `set_budget_caps` con 1 yocto (una FullAccess key del owner).

use crate::authorize::{Fuente, cabe};
use crate::error::ArrasErrorV2;
use crate::events::{ArrasEventV2, BudgetAmountData, BudgetCapsData};
use crate::msg::{OperatorTicket, parse_hex_16, parse_hex_32};
use crate::ticket::TicketSubject;
use crate::*;
use arras_escrow_core::AuthorizeMsg;
use near_sdk::assert_one_yocto;

#[near]
impl Arras {
    /// El agente autoriza un pago desde el presupuesto `(owner, predecessor, operator_id)`. El `payment_id` es el de la
    /// sec. 7 con `payer = owner` (ver `compute_budget_payment_id`). `dispute_deadline` ausente = `reclaim_after`.
    /// Devuelve el `payment_id`.
    #[allow(clippy::too_many_arguments)]
    pub fn authorize_from_budget(
        &mut self,
        owner: AccountId,
        operator_id: OperatorId,
        receiver: AccountId,
        amount: U128,
        release_deadline: U64,
        reclaim_after: U64,
        dispute_deadline: Option<U64>,
        fee_bps_max: u16,
        terms_hash: String,
        salt: String,
        operator_ticket: Option<TicketArg>,
    ) -> PaymentId {
        let agent = env::predecessor_account_id();
        if self.paused {
            ArrasError::E02Pausado.panic();
        }
        let (Some(terms), Some(salt)) = (parse_hex_32(&terms_hash), parse_hex_16(&salt)) else {
            ArrasError::E03MsgInvalido.panic()
        };
        let m = AuthorizeMsg {
            operator_id,
            receiver,
            release_deadline: release_deadline.0,
            reclaim_after: reclaim_after.0,
            fee_bps_max,
            terms_hash: terms,
            salt,
        };
        let ticket = operator_ticket.map(OperatorTicket::from);
        let plan = self
            .plan_authorize_from_budget(
                &owner,
                &agent,
                amount.0,
                &m,
                dispute_deadline.map_or(m.reclaim_after, |d| d.0),
                ticket.as_ref(),
            )
            .unwrap_or_else(|e| e.panic());
        let payment_id = plan.payment_id.clone();
        self.apply_authorize(plan);
        payment_id
    }

    /// El owner pasa todo el saldo a su `claimable`. El presupuesto queda (en 0) con sus topes. Funciona en pausa.
    pub fn withdraw_budget(&mut self, agent: AccountId, operator_id: OperatorId) {
        let owner = env::predecessor_account_id();
        let key: BudgetKey = (owner, agent, operator_id);
        let mut b = self.budget_or_e50(&key);
        let saldo = b.saldo.0;
        if saldo == 0 {
            ArrasError::E30SinSaldo.panic();
        }
        b.saldo = U128(0);
        self.budget_to_owner(&key, saldo);
        self.budgets.insert(key.clone(), b);
        let (owner, agent, operator_id) = key;
        ArrasEventV2::BudgetWithdrawn(vec![BudgetAmountData {
            owner,
            agent,
            operator_id,
            amount: U128(saldo),
        }])
        .emit();
    }

    /// El owner revoca al agente: el saldo va a su `claimable`, el presupuesto se borra y su storage vuelve a quien lo
    /// pago. Funciona en pausa. Los pagos que el agente ya autorizo siguen su curso (son del owner).
    pub fn revoke_agent(&mut self, agent: AccountId, operator_id: OperatorId) {
        let owner = env::predecessor_account_id();
        let key: BudgetKey = (owner, agent, operator_id);
        let b = self.budget_or_e50(&key);
        let saldo = b.saldo.0;
        if saldo > 0 {
            self.budget_to_owner(&key, saldo);
        }
        self.budgets.flush();
        let antes = env::storage_usage();
        self.budgets.remove(&key);
        self.budgets.flush();
        let liberado = bytes_cost(antes.saturating_sub(env::storage_usage()));
        let devuelto = liberado.min(b.storage_charged.0);
        let (owner, agent, operator_id) = key;
        self.refund_storage(&operator_id, &owner, b.sponsored, devuelto);
        ArrasEventV2::AgentRevoked(vec![BudgetAmountData {
            owner,
            agent,
            operator_id,
            amount: U128(saldo),
        }])
        .emit();
    }

    /// El owner fija los topes (puede subirlos), con 1 yocto: una function-call key no lo puede firmar.
    #[payable]
    pub fn set_budget_caps(
        &mut self,
        agent: AccountId,
        operator_id: OperatorId,
        tope_por_pago: U128,
        tope_diario: U128,
    ) {
        assert_one_yocto();
        let owner = env::predecessor_account_id();
        let key: BudgetKey = (owner, agent, operator_id);
        let mut b = self.budget_or_e50(&key);
        if tope_por_pago.0 == 0 || tope_diario.0 == 0 {
            ArrasErrorV2::E56TopesPresupuesto.panic();
        }
        b.tope_por_pago = tope_por_pago;
        b.tope_diario = tope_diario;
        self.budgets.insert(key.clone(), b);
        let (owner, agent, operator_id) = key;
        ArrasEventV2::BudgetCapsSet(vec![BudgetCapsData {
            owner,
            agent,
            operator_id,
            tope_por_pago,
            tope_diario,
        }])
        .emit();
    }
}

impl Arras {
    fn budget_or_e50(&self, key: &BudgetKey) -> Budget {
        self.budgets
            .get(key)
            .cloned()
            .unwrap_or_else(|| ArrasErrorV2::E50SinPresupuesto.panic())
    }

    /// El saldo sale del presupuesto al `claimable` del owner: baja `budgets_total`, `tvl_operador` y `payer_tvl`
    /// (no `tvl_global`: la plata sigue en el contrato hasta el retiro).
    fn budget_to_owner(&mut self, key: &BudgetKey, saldo: u128) {
        let (owner, _, operator_id) = key;
        self.budgets_total = sub(self.budgets_total, saldo);
        self.leave_live(operator_id, owner, saldo);
        self.credit(owner, saldo);
    }

    /// Las guardas de `authorize_from_budget` despues de E02 y E03, sin escribir nada.
    pub(crate) fn plan_authorize_from_budget(
        &self,
        owner: &AccountId,
        agent: &AccountId,
        amount: u128,
        m: &AuthorizeMsg,
        dispute_deadline: u64,
        ticket: Option<&OperatorTicket>,
    ) -> Result<crate::authorize::AuthorizePlan, ArrasErrorV2> {
        let operator = self
            .operators
            .get(&m.operator_id)
            .ok_or(ArrasErrorV2::V1(ArrasError::E04OperadorInexistente))?;
        let key: BudgetKey = (owner.clone(), agent.clone(), m.operator_id.clone());
        let mut b = self
            .budgets
            .get(&key)
            .cloned()
            .ok_or(ArrasErrorV2::E50SinPresupuesto)?;
        if m.receiver == env::current_account_id() || &m.receiver == owner || &m.receiver == agent {
            return Err(ArrasError::E15ReceiverInvalido.into());
        }
        // Un pago de 0 no pasa por el token (aca no hay `ft_transfer_call`): con `min_amount = 0` el agente crearia pagos
        // vacios que gastan el storage del owner (refutacion de v2, P3-2).
        if amount == 0 {
            return Err(ArrasError::E24Monto.into());
        }
        if amount < operator.min_amount.0 {
            return Err(ArrasError::E05DebajoDelMinimo.into());
        }
        if amount > self.tope_por_pago {
            return Err(ArrasError::E06TopePorPago.into());
        }
        if amount > b.tope_por_pago.0 {
            return Err(ArrasErrorV2::E52TopePorPagoPresupuesto);
        }
        let hoy = env::block_timestamp() / DAY_NS;
        let gastado = if b.dia.0 == hoy { b.gastado_hoy.0 } else { 0 };
        let gastado = cabe(
            gastado,
            amount,
            b.tope_diario.0,
            ArrasErrorV2::E51TopeDiario,
        )?;
        if amount > b.saldo.0 {
            return Err(ArrasErrorV2::E50SinPresupuesto);
        }
        // El agente elige los plazos por el owner: sin horizonte, un `reclaim_after` lejano trabaria la plata del owner
        // para siempre (refutacion de v2, P1-1). Con el horizonte, el owner la recupera con `reclaim` a lo sumo
        // `MAX_PLAZO_PRESUPUESTO_NS` despues de autorizada (mas el plazo del arbitro si el receiver la disputa).
        if m.reclaim_after > env::block_timestamp().saturating_add(MAX_PLAZO_PRESUPUESTO_NS) {
            return Err(ArrasErrorV2::E57PlazoPresupuesto);
        }
        let (payment_id, payment) =
            self.build_payment(m, owner, amount, dispute_deadline, operator)?;
        self.check_ticket(
            operator,
            TicketSubject::Payment {
                payment_id: &payment_id,
            },
            ticket,
        )?;
        b.saldo = U128(b.saldo.0 - amount);
        b.gastado_hoy = U128(gastado);
        b.dia = U64(hoy);
        let tvl_key = (m.operator_id.clone(), owner.clone());
        self.plan_storage(
            payment_id,
            payment,
            tvl_key,
            Fuente::Presupuesto {
                key,
                budget: b,
                amount,
            },
            operator,
        )
    }
}
