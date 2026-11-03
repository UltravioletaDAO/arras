//! Disputa (prescripcion sec. 4; plan v2 sec. 3.3 y F-13). Sin promesas, como el resto del ciclo de vida, y abierta
//! en pausa (proteger y cerrar disputas no mete plata nueva).
//!
//! | Metodo | Quien | Guardas en orden |
//! |---|---|---|
//! | `open_dispute(payment_id, reason_hash)` | el payer o el receiver | E20 -> E60 -> E22 (no vivo) -> E61 (`now >= dispute_deadline`) -> E66 |
//! | `resolve_dispute(payment_id, to_receiver, to_payer)` | el arbitro **actual** del operador | E20 -> E62 -> E22 (no `Disputed`) -> E65 (vencio su plazo) -> E63 (`to_receiver + to_payer != vivo`) |
//! | `resolve_expired_dispute(payment_id)` | cualquiera | E20 -> E22 (no `Disputed`) -> E64 (el arbitro todavia esta en plazo) |
//!
//! Abrir congela lo vivo (`Disputed`: release, refund y reclaim dan `E22`). Resolver reparte lo vivo: `to_receiver`
//! como un `release` (la tarifa congelada sale de ahi), `to_payer` al payer; el pago queda `Resolved` (terminal).
//!
//! **Vencido el plazo del arbitro** (`opened_at + arbiter_window_ns`, el plazo congelado al autorizar), la disputa se
//! resuelve al ultimo estado acordado: lo liberado antes queda liberado y lo vivo va a la parte que **no** abrio la
//! disputa. Asi abrirla no es gratis: un payer que la abre para trabar un pago terminado no recupera nada si el
//! arbitro no aparece (griefing al worker, plan sec. 3.7), y un receiver que la abre para frenar un refund tampoco
//! cobra por eso. Es la regla de la prescripcion sec. 4 ("quien disputa carga el riesgo del plazo"), revisable por
//! el dueno.
//!
//! **Salvo** que la haya abierto el payer en o despues de `release_deadline` (`opened_at >= release_deadline`): ahi el
//! capturer ya no podia liberar (`E23`), el ultimo estado acordado es la devolucion, y lo vivo vuelve al payer. Sin esa
//! excepcion, un payer que disputaba un pago que nadie aprobo (o el owner de un presupuesto, un pago de la llave del
//! agente filtrada) le regalaba lo vivo al receiver si el arbitro no contestaba (refutacion RARRAS-6, P2-1; decision de
//! c0der). El caso que la regla protege, disputar antes de `release_deadline` para trabar un trabajo terminado, no
//! cambia.

use crate::error::ArrasErrorV2;
use crate::events::{ArrasEventV2, DisputeOpenedData, DisputeResolvedData};
use crate::msg::parse_hex_32;
use crate::*;
use arras_escrow_core::fee_split;

#[near]
impl Arras {
    /// El payer o el receiver, antes de `dispute_deadline`, con el pago vivo. `reason_hash`: hex de 64 en minusculas
    /// (el motivo vive fuera de la cadena).
    pub fn open_dispute(&mut self, payment_id: PaymentId, reason_hash: String) {
        let mut p = self.payment_or_e20(&payment_id);
        let caller = env::predecessor_account_id();
        let party = if caller == p.payer {
            Party::Payer
        } else if caller == p.receiver {
            Party::Receiver
        } else {
            ArrasErrorV2::E60NoEsParte.panic()
        };
        if !p.status.is_live() {
            ArrasError::E22Estado("open_dispute").panic();
        }
        let now = env::block_timestamp();
        if now >= p.dispute_deadline.0 {
            ArrasErrorV2::E61FueraDePlazoDisputa.panic();
        }
        if parse_hex_32(&reason_hash).is_none() {
            ArrasErrorV2::E66ReasonHash.panic();
        }
        let amount = p.live_amount();
        let resolve_by = now.saturating_add(p.arbiter_window_ns.0);
        p.status = PaymentStatus::Disputed;
        // Esa parte de la reserva ya cobrada pasa a estar en uso (el `Payment` crece ahora 78 bytes; los otros 8, al
        // resolver).
        self.dispute_reserve_total =
            sub(self.dispute_reserve_total, bytes_cost(DISPUTE_OPEN_BYTES));
        p.dispute = Some(Dispute {
            opened_by: party,
            reason_hash: reason_hash.clone(),
            opened_at: U64(now),
            resolved_at: None,
        });
        self.payments.insert(payment_id.clone(), p);
        ArrasEventV2::DisputeOpened(vec![DisputeOpenedData {
            payment_id,
            opened_by: caller,
            reason_hash,
            amount: U128(amount),
            resolve_by: U64(resolve_by),
        }])
        .emit();
    }

    /// Solo el arbitro actual del operador, dentro de su plazo. `to_receiver + to_payer` tiene que ser exactamente
    /// lo vivo; la tarifa congelada se cobra sobre `to_receiver`.
    pub fn resolve_dispute(&mut self, payment_id: PaymentId, to_receiver: U128, to_payer: U128) {
        let p = self.payment_or_e20(&payment_id);
        let caller = env::predecessor_account_id();
        let es_arbitro = self
            .operators
            .get(&p.operator_id)
            .is_some_and(|o| o.arbiter == caller);
        if !es_arbitro {
            ArrasErrorV2::E62NoEsArbitro.panic();
        }
        let resolve_by = Self::disputed_until(&p, "resolve_dispute");
        if env::block_timestamp() >= resolve_by {
            ArrasErrorV2::E65ArbitroVencido.panic();
        }
        if to_receiver.0.checked_add(to_payer.0) != Some(p.live_amount()) {
            ArrasErrorV2::E63RepartoInvalido.panic();
        }
        self.settle_dispute(payment_id, p, to_receiver.0, to_payer.0, caller, false);
    }

    /// Cualquiera, vencido el plazo del arbitro: lo vivo va a la parte que no abrio la disputa, salvo que la haya
    /// abierto el payer en o despues de `release_deadline` (entonces vuelve al payer).
    pub fn resolve_expired_dispute(&mut self, payment_id: PaymentId) {
        let p = self.payment_or_e20(&payment_id);
        let resolve_by = Self::disputed_until(&p, "resolve_expired_dispute");
        if env::block_timestamp() < resolve_by {
            ArrasErrorV2::E64ArbitroEnPlazo.panic();
        }
        let live = p.live_amount();
        // Cobra el receiver solo si la abrio el payer cuando el capturer todavia podia liberar.
        let cobra_el_receiver = p
            .dispute
            .as_ref()
            .is_some_and(|d| d.opened_by == Party::Payer && d.opened_at.0 < p.release_deadline.0);
        let (to_receiver, to_payer) = if cobra_el_receiver {
            (live, 0)
        } else {
            (0, live)
        };
        self.settle_dispute(
            payment_id,
            p,
            to_receiver,
            to_payer,
            env::predecessor_account_id(),
            true,
        );
    }
}

impl Arras {
    /// `opened_at + arbiter_window_ns` de un pago `Disputed`; si no esta en disputa, `E22` con el nombre de `op`.
    fn disputed_until(p: &Payment, op: &'static str) -> u64 {
        match (&p.status, &p.dispute) {
            (PaymentStatus::Disputed, Some(d)) => {
                d.opened_at.0.saturating_add(p.arbiter_window_ns.0)
            }
            _ => ArrasError::E22Estado(op).panic(),
        }
    }

    /// Reparte lo vivo y deja el pago `Resolved`.
    fn settle_dispute(
        &mut self,
        payment_id: PaymentId,
        mut p: Payment,
        to_receiver: u128,
        to_payer: u128,
        by: AccountId,
        expired: bool,
    ) {
        let live = p.live_amount();
        let split = fee_split(to_receiver, p.fee_bps).unwrap_or_else(|| env::abort());
        p.released = U128(add(p.released.0, to_receiver));
        p.refunded = U128(add(p.refunded.0, to_payer));
        p.status = PaymentStatus::Resolved;
        if let Some(d) = p.dispute.as_mut() {
            d.resolved_at = Some(U64(env::block_timestamp()));
            self.dispute_reserve_total = sub(
                self.dispute_reserve_total,
                bytes_cost(DISPUTE_RESOLVE_BYTES),
            );
        }
        self.credit(&p.receiver, split.net);
        self.credit(&p.fee_recipient, split.fee);
        self.credit(&p.payer, to_payer);
        self.leave_live(&p.operator_id, &p.payer, live);
        self.payments.insert(payment_id.clone(), p);
        ArrasEventV2::DisputeResolved(vec![DisputeResolvedData {
            payment_id,
            by,
            to_receiver: U128(to_receiver),
            net: U128(split.net),
            fee: U128(split.fee),
            to_payer: U128(to_payer),
            expired,
        }])
        .emit();
    }
}
