//! `ft_on_transfer` de v2 (prescripcion sec. 3.3, 3.4 y 4). **Nunca entra en panico**: un rechazo es
//! `Value(amount)` + el log `arras:rechazo:<CODIGO>` y el estado queda intacto, porque las guardas corren sobre
//! `&self` (`plan_*`) antes de la primera escritura (`apply_*`). Dos `msg`:
//!
//! - `authorize` (el de v1 mas `dispute_deadline` y `operator_ticket` opcionales). Guardas: E01 -> E02 -> E03 -> E04
//!   -> E15 -> E05 -> E06 -> E08 -> E09 -> E10 -> E11 (con `now < dispute_deadline <= reclaim_after`) -> E12 -> E13
//!   -> E53/E54 (ticket, si el operador tiene llave) -> E14.
//! - `budget` (deposito en el presupuesto `(sender_id, agent, operator_id)`). Guardas: E01 -> E02 -> E03 -> E04 ->
//!   E55 (agente = el contrato) -> E56 (un tope en 0) -> E05 -> E08 -> E09 -> E10 -> E53/E54 -> E14 (solo si el
//!   presupuesto es nuevo). El deposito sube los tres TVL; un re-deposito solo puede bajar los topes (`min`).
//!
//! `authorize_from_budget` ([`crate::budget`]) arma su pago con las mismas piezas ([`Arras::build_payment`],
//! [`Arras::check_ticket`], [`Arras::plan_storage`]) y lo escribe con [`Arras::apply_authorize`].
//!
//! Storage (sec. 3.6): igual que v1, mas [`DISPUTE_RESERVE_BYTES`] por pago (lo que crece el `Payment` si se disputa,
//! porque quien abre la disputa no adjunta deposito). Primero el saldo NEP-145 del payer; si no alcanza, el
//! `sponsor_pool` del operador hasta `sponsor_max_por_payer`.

use crate::error::ArrasErrorV2;
use crate::events::{ArrasEventV2, BudgetDepositedData, BudgetSpentData};
use crate::msg::{AuthorizeMsgV2, BudgetMsg, FtMsg, OperatorTicket, parse_ft_msg};
use crate::ticket::{TicketSubject, verify_ticket};
use crate::*;
use arras_escrow_core::events::{ArrasEvent, AuthorizedData};
use arras_escrow_core::{AuthorizeMsg, PaymentIdInput, compute_payment_id as core_payment_id};
use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;
use near_sdk::{PromiseOrValue, borsh};

type R<T> = Result<T, ArrasErrorV2>;

/// De donde sale la plata de un pago nuevo.
pub(crate) enum Fuente {
    /// `ft_transfer_call`: la plata entra al contrato y suben los tres TVL.
    Ft {
        new_tvl_global: u128,
        new_tvl_operador: u128,
        new_payer_tvl: u128,
    },
    /// `authorize_from_budget`: la plata ya estaba en el presupuesto (y en los TVL); solo baja el saldo.
    Presupuesto {
        key: BudgetKey,
        budget: Budget,
        amount: u128,
    },
}

/// Todo lo que un pago nuevo escribe, calculado y validado antes de escribir.
pub(crate) struct AuthorizePlan {
    pub payment_id: PaymentId,
    pub payment: Payment,
    pub tvl_key: (OperatorId, AccountId),
    pub fuente: Fuente,
    pub new_claimable: Vec<AccountId>,
    /// `Some(nuevo_sponsored_used)` si paga el patrocinio.
    pub sponsor: Option<u128>,
}

/// Todo lo que un deposito de presupuesto escribe.
pub(crate) struct BudgetDepositPlan {
    pub key: BudgetKey,
    pub budget: Budget,
    pub amount: u128,
    pub new_tvl_global: u128,
    pub new_tvl_operador: u128,
    pub new_payer_tvl: u128,
    /// La entrada `claimable[owner]` es nueva.
    pub new_claimable_owner: bool,
    /// `Some(nuevo_sponsored_used)` si el patrocinio paga el registro.
    pub sponsor: Option<u128>,
}

pub(crate) enum FtPlan {
    /// En una caja: el plan de un pago es mucho mas grande que el de un deposito (clippy `large_enum_variant`).
    Authorize(Box<AuthorizePlan>),
    Budget(BudgetDepositPlan),
}

fn rechazo_si(cond: bool, e: impl Into<ArrasErrorV2>) -> R<()> {
    if cond { Err(e.into()) } else { Ok(()) }
}

/// `actual + amount <= tope`, sin desbordar (desborde = supera el tope).
pub(crate) fn cabe(actual: u128, amount: u128, tope: u128, e: impl Into<ArrasErrorV2>) -> R<u128> {
    match actual.checked_add(amount) {
        Some(n) if n <= tope => Ok(n),
        _ => Err(e.into()),
    }
}

#[near]
impl FungibleTokenReceiver for Arras {
    /// Solo `token`. `Value(0)` si autoriza o deposita; `Value(amount)` y `arras:rechazo:<CODIGO>` si no.
    fn ft_on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        match self.plan_ft(&sender_id, amount.0, &msg) {
            Ok(FtPlan::Authorize(plan)) => {
                self.apply_authorize(*plan);
                PromiseOrValue::Value(U128(0))
            }
            Ok(FtPlan::Budget(plan)) => {
                self.apply_budget_deposit(plan);
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
    /// E01 -> E02 -> E03 y despues las guardas del `msg` que llego.
    pub(crate) fn plan_ft(&self, sender: &AccountId, amount: u128, msg: &str) -> R<FtPlan> {
        rechazo_si(
            env::predecessor_account_id() != self.token,
            ArrasError::E01TokenNoPermitido,
        )?;
        rechazo_si(self.paused, ArrasError::E02Pausado)?;
        match parse_ft_msg(msg)? {
            FtMsg::Authorize(m) => self
                .plan_authorize(sender, amount, &m)
                .map(|p| FtPlan::Authorize(Box::new(p))),
            FtMsg::Budget(m) => self
                .plan_budget_deposit(sender, amount, &m)
                .map(FtPlan::Budget),
        }
    }

    fn operator_or_e04(&self, operator_id: &str) -> R<&Operator> {
        self.operators
            .get(operator_id)
            .ok_or(ArrasError::E04OperadorInexistente.into())
    }

    /// authorize por `ft_transfer_call`: E04 -> E15 -> E05 -> E06 -> E08 -> E09 -> E10 -> E11 -> E12 -> E13 -> ticket
    /// -> E14.
    fn plan_authorize(
        &self,
        payer: &AccountId,
        amount: u128,
        msg: &AuthorizeMsgV2,
    ) -> R<AuthorizePlan> {
        let m = &msg.base;
        let operator = self.operator_or_e04(&m.operator_id)?;
        rechazo_si(
            m.receiver == env::current_account_id() || &m.receiver == payer,
            ArrasError::E15ReceiverInvalido,
        )?;
        rechazo_si(
            amount < operator.min_amount.0,
            ArrasError::E05DebajoDelMinimo,
        )?;
        rechazo_si(amount > self.tope_por_pago, ArrasError::E06TopePorPago)?;
        let tvl_key = (m.operator_id.clone(), payer.clone());
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
        let (payment_id, payment) =
            self.build_payment(m, payer, amount, msg.effective_dispute_deadline(), operator)?;
        self.check_ticket(
            operator,
            TicketSubject::Payment {
                payment_id: &payment_id,
            },
            msg.operator_ticket.as_ref(),
        )?;
        self.plan_storage(
            payment_id,
            payment,
            tvl_key,
            Fuente::Ft {
                new_tvl_global,
                new_tvl_operador,
                new_payer_tvl,
            },
            operator,
        )
    }

    /// E11 -> E12 -> E13 y el `Payment` nuevo (sin storage cobrado todavia). Lo comparten el `ft_transfer_call` y
    /// `authorize_from_budget`.
    pub(crate) fn build_payment(
        &self,
        m: &AuthorizeMsg,
        payer: &AccountId,
        amount: u128,
        dispute_deadline: u64,
        operator: &Operator,
    ) -> R<(PaymentId, Payment)> {
        let now = env::block_timestamp();
        rechazo_si(
            !(now < m.release_deadline
                && m.release_deadline < m.reclaim_after
                && now < dispute_deadline
                && dispute_deadline <= m.reclaim_after),
            ArrasError::E11Plazos,
        )?;
        rechazo_si(
            operator.fee_bps > m.fee_bps_max,
            ArrasError::E12FeeMayorAlMaximo,
        )?;
        let (hash, payment_id) = core_payment_id(&PaymentIdInput::from_msg(
            m,
            self.token.as_str(),
            payer.as_str(),
            amount,
        ));
        rechazo_si(
            self.payments.contains_key(&payment_id) || self.used_ids.contains(&hash),
            ArrasError::E13IdUsado,
        )?;
        let payment = Payment {
            operator_id: m.operator_id.clone(),
            payer: payer.clone(),
            receiver: m.receiver.clone(),
            amount: U128(amount),
            released: U128(0),
            refunded: U128(0),
            fee_bps: operator.fee_bps,
            fee_recipient: operator.fee_recipient.clone(),
            fee_bps_max: m.fee_bps_max,
            created_at: U64(now),
            release_deadline: U64(m.release_deadline),
            reclaim_after: U64(m.reclaim_after),
            terms_hash: crate::msg::hex_encode_32(&m.terms_hash),
            status: PaymentStatus::Authorized,
            storage_charged: U128(0),
            sponsored: false,
            dispute_deadline: U64(dispute_deadline),
            arbiter_window_ns: operator.arbiter_window_ns,
            dispute: None,
        };
        Ok((payment_id, payment))
    }

    /// Si el operador tiene `ticket_pubkey`: hace falta un ticket (`E53` si falta), sin vencer (`E54`) y con la firma
    /// del sujeto (`E53`). Si no tiene llave, un ticket que llega igual es `E53` (nadie lo puede verificar).
    pub(crate) fn check_ticket(
        &self,
        operator: &Operator,
        subject: TicketSubject<'_>,
        ticket: Option<&OperatorTicket>,
    ) -> R<()> {
        match (operator.ticket_key_bytes(), ticket) {
            (None, None) => Ok(()),
            (None, Some(_)) | (Some(_), None) => Err(ArrasErrorV2::E53TicketInvalido),
            (Some(pk), Some(t)) => {
                rechazo_si(
                    env::block_timestamp() >= t.expiry,
                    ArrasErrorV2::E54TicketVencido,
                )?;
                rechazo_si(
                    !verify_ticket(&pk, env::current_account_id().as_str(), subject, t),
                    ArrasErrorV2::E53TicketInvalido,
                )
            }
        }
    }

    /// E14 y el plan completo: el costo exacto de lo que escribe el pago (mas la reserva de la disputa), primero del
    /// saldo NEP-145 del payer y si no del patrocinio del operador.
    pub(crate) fn plan_storage(
        &self,
        payment_id: PaymentId,
        mut payment: Payment,
        tvl_key: (OperatorId, AccountId),
        fuente: Fuente,
        operator: &Operator,
    ) -> R<AuthorizePlan> {
        let mut new_claimable: Vec<AccountId> = Vec::with_capacity(3);
        for a in [&payment.receiver, &payment.fee_recipient, &payment.payer] {
            if !new_claimable.contains(a) && !self.claimable.contains_key(a) {
                new_claimable.push(a.clone());
            }
        }
        let bytes = self.authorize_bytes(&payment_id, &payment, &tvl_key, &new_claimable);
        let (cost, sponsor) = self.charge_storage(
            &payment.payer,
            &tvl_key,
            bytes.base,
            bytes.sponsor_extra,
            operator,
        )?;
        payment.storage_charged = U128(cost);
        payment.sponsored = sponsor.is_some();
        Ok(AuthorizePlan {
            payment_id,
            payment,
            tvl_key,
            fuente,
            new_claimable,
            sponsor,
        })
    }

    /// `(costo, Some(nuevo_sponsored_used) si paga el patrocinio)`: el saldo NEP-145 de `payer` si alcanza para
    /// `base`; si no, el `sponsor_pool` para `base + sponsor_extra` hasta `sponsor_max_por_payer`. Si no, `E14`.
    pub(crate) fn charge_storage(
        &self,
        payer: &AccountId,
        tvl_key: &(OperatorId, AccountId),
        base: u64,
        sponsor_extra: u64,
        operator: &Operator,
    ) -> R<(u128, Option<u128>)> {
        let cost_payer = bytes_cost(base);
        let payer_available = self.storage.get(payer).map_or(0, |e| e.available);
        if payer_available >= cost_payer {
            return Ok((cost_payer, None));
        }
        let cost = bytes_cost(base + sponsor_extra);
        let used = self.sponsored_used.get(tvl_key).copied().unwrap_or(0);
        let new_used = cabe(
            used,
            cost,
            operator.sponsor_max_por_payer.0,
            ArrasError::E14SinStorage,
        )?;
        rechazo_si(operator.sponsor_pool.0 < cost, ArrasError::E14SinStorage)?;
        Ok((cost, Some(new_used)))
    }

    /// Bytes que escribe el pago (layout de `near_sdk::store` 5.29.1, como v1) mas [`DISPUTE_RESERVE_BYTES`].
    pub(crate) fn authorize_bytes(
        &self,
        payment_id: &str,
        payment: &Payment,
        tvl_key: &(OperatorId, AccountId),
        new_claimable: &[AccountId],
    ) -> crate::storage::Bytes {
        let id = payment_id.len() as u64;
        let payment_len = borsh::to_vec(payment)
            .map(|v| v.len())
            .unwrap_or_else(|_| env::abort()) as u64;
        let mut base = record_bytes(2 + 4, 4 + id)
            + record_bytes(32, payment_len + 4)
            + record_bytes(index_prefix(&payment.operator_id).len() as u64 + 4, 4 + id)
            + DISPUTE_RESERVE_BYTES;
        base += self.pair_and_claimable_bytes(tvl_key, new_claimable);
        crate::storage::Bytes {
            base,
            sponsor_extra: self.sponsor_extra_bytes(tvl_key),
        }
    }

    /// Escribe un pago nuevo. No falla: todo se valido en el plan.
    pub(crate) fn apply_authorize(&mut self, plan: AuthorizePlan) {
        let AuthorizePlan {
            payment_id,
            payment,
            tvl_key,
            fuente,
            new_claimable,
            sponsor,
        } = plan;
        let cost = payment.storage_charged.0;
        self.pay_storage(&payment.payer, &tvl_key, cost, sponsor);
        let operator = self
            .operators
            .get_mut(&payment.operator_id)
            .unwrap_or_else(|| env::abort());
        let fee_recipient = operator.fee_recipient.clone();
        let mut gasto = None;
        match fuente {
            Fuente::Ft {
                new_tvl_global,
                new_tvl_operador,
                new_payer_tvl,
            } => {
                operator.tvl_operador = U128(new_tvl_operador);
                self.tvl_global = new_tvl_global;
                self.payer_tvl.insert(tvl_key, new_payer_tvl);
            }
            Fuente::Presupuesto {
                key,
                budget,
                amount,
            } => {
                self.budgets_total = sub(self.budgets_total, amount);
                gasto = Some((key.clone(), budget.saldo, budget.gastado_hoy));
                self.budgets.insert(key, budget);
            }
        }
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
        if let Some(((owner, agent, operator_id), saldo, gastado_hoy)) = gasto {
            ArrasEventV2::BudgetSpent(vec![BudgetSpentData {
                owner,
                agent,
                operator_id,
                payment_id: payment_id.clone(),
                amount: payment.amount,
                saldo,
                gastado_hoy,
            }])
            .emit();
        }
        self.payments.insert(payment_id, payment);
    }

    /// Deposito de presupuesto: E04 -> E55 -> E56 -> E05 -> E08 -> E09 -> E10 -> ticket -> E14.
    fn plan_budget_deposit(
        &self,
        owner: &AccountId,
        amount: u128,
        msg: &BudgetMsg,
    ) -> R<BudgetDepositPlan> {
        let operator = self.operator_or_e04(&msg.operator_id)?;
        rechazo_si(
            msg.agent == env::current_account_id(),
            ArrasErrorV2::E55AgenteInvalido,
        )?;
        rechazo_si(
            msg.tope_por_pago == 0 || msg.tope_diario == 0,
            ArrasErrorV2::E56TopesPresupuesto,
        )?;
        rechazo_si(
            amount < operator.min_amount.0,
            ArrasError::E05DebajoDelMinimo,
        )?;
        let tvl_key = (msg.operator_id.clone(), owner.clone());
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
        self.check_ticket(
            operator,
            TicketSubject::Budget {
                operator_id: &msg.operator_id,
                owner: owner.as_str(),
                agent: msg.agent.as_str(),
            },
            msg.operator_ticket.as_ref(),
        )?;
        let key: BudgetKey = (owner.clone(), msg.agent.clone(), msg.operator_id.clone());
        let new_claimable_owner = !self.claimable.contains_key(owner);
        let (budget, sponsor) = match self.budgets.get(&key) {
            Some(b) => {
                let mut b = b.clone();
                b.saldo = U128(add(b.saldo.0, amount));
                b.tope_por_pago = U128(b.tope_por_pago.0.min(msg.tope_por_pago));
                b.tope_diario = U128(b.tope_diario.0.min(msg.tope_diario));
                (b, None)
            }
            None => {
                let mut b = Budget {
                    saldo: U128(amount),
                    tope_por_pago: U128(msg.tope_por_pago),
                    tope_diario: U128(msg.tope_diario),
                    gastado_hoy: U128(0),
                    dia: U64(env::block_timestamp() / DAY_NS),
                    storage_charged: U128(0),
                    sponsored: false,
                };
                let claimable_nuevo: Vec<AccountId> = if new_claimable_owner {
                    vec![owner.clone()]
                } else {
                    vec![]
                };
                let base = self.budget_record_bytes(&key)
                    + self.pair_and_claimable_bytes(&tvl_key, &claimable_nuevo);
                let (cost, sponsor) = self.charge_storage(
                    owner,
                    &tvl_key,
                    base,
                    self.sponsor_extra_bytes(&tvl_key),
                    operator,
                )?;
                b.storage_charged = U128(cost);
                b.sponsored = sponsor.is_some();
                (b, sponsor)
            }
        };
        Ok(BudgetDepositPlan {
            key,
            budget,
            amount,
            new_tvl_global,
            new_tvl_operador,
            new_payer_tvl,
            new_claimable_owner,
            sponsor,
        })
    }

    /// Escribe un deposito de presupuesto. No falla.
    fn apply_budget_deposit(&mut self, plan: BudgetDepositPlan) {
        let BudgetDepositPlan {
            key,
            budget,
            amount,
            new_tvl_global,
            new_tvl_operador,
            new_payer_tvl,
            new_claimable_owner,
            sponsor,
        } = plan;
        let (owner, agent, operator_id) = key.clone();
        let tvl_key = (operator_id.clone(), owner.clone());
        if !self.budgets.contains_key(&key) {
            self.pay_storage(&owner, &tvl_key, budget.storage_charged.0, sponsor);
        }
        let operator = self
            .operators
            .get_mut(&operator_id)
            .unwrap_or_else(|| env::abort());
        operator.tvl_operador = U128(new_tvl_operador);
        self.tvl_global = new_tvl_global;
        self.payer_tvl.insert(tvl_key, new_payer_tvl);
        if new_claimable_owner {
            self.claimable.insert(owner.clone(), 0);
        }
        self.budgets_total = add(self.budgets_total, amount);
        ArrasEventV2::BudgetDeposited(vec![BudgetDepositedData {
            owner,
            agent,
            operator_id,
            amount: U128(amount),
            saldo: budget.saldo,
            tope_por_pago: budget.tope_por_pago,
            tope_diario: budget.tope_diario,
        }])
        .emit();
        self.budgets.insert(key, budget);
    }

    /// Descuenta `cost` de donde corresponde: el patrocinio (`Some(nuevo_used)`) o el saldo NEP-145 del payer.
    pub(crate) fn pay_storage(
        &mut self,
        payer: &AccountId,
        tvl_key: &(OperatorId, AccountId),
        cost: u128,
        sponsor: Option<u128>,
    ) {
        match sponsor {
            Some(new_used) => {
                let operator = self
                    .operators
                    .get_mut(&tvl_key.0)
                    .unwrap_or_else(|| env::abort());
                operator.sponsor_pool = U128(sub(operator.sponsor_pool.0, cost));
                self.sponsored_used.insert(tvl_key.clone(), new_used);
            }
            None => {
                let entry = self.storage.get_mut(payer).unwrap_or_else(|| env::abort());
                entry.available = sub(entry.available, cost);
                self.storage_available_total = sub(self.storage_available_total, cost);
            }
        }
    }
}
