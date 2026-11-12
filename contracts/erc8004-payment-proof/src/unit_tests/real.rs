//! El espejo contra el `Payment` de verdad (`arras_escrow_v2::Payment`, solo en los tests): todo lo que el escrow
//! serializa, en cualquiera de sus estados, deserializa campo por campo en `EscrowPayment`, y `check_payment` decide
//! lo mismo que el modelo de la decision (`released > 0`, la disputa vencida a favor del receiver no, `payer !=
//! receiver`, `client` es parte).

use super::*;
use arras_escrow_v2::{Dispute, Party, Payment, PaymentStatus};
use proptest::prelude::*;

fn hex64(b: u8) -> String {
    format!("{b:02x}").repeat(32)
}

#[allow(clippy::too_many_arguments)]
fn real(
    payer: &str,
    receiver: &str,
    status: PaymentStatus,
    amount: u128,
    released: u128,
    refunded: u128,
    dispute: Option<Dispute>,
) -> Payment {
    Payment {
        operator_id: "em".into(),
        payer: acc(payer),
        receiver: acc(receiver),
        amount: U128(amount),
        released: U128(released),
        refunded: U128(refunded),
        fee_bps: 1300,
        fee_recipient: acc("em-fees.arras-dev.testnet"),
        fee_bps_max: 1300,
        created_at: U64(1_793_491_200_000_000_000),
        release_deadline: U64(1_793_577_600_000_000_000),
        reclaim_after: U64(1_793_664_000_000_000_000),
        terms_hash: hex64(0x77),
        status,
        storage_charged: U128(4_630_000_000_000_000_000),
        sponsored: true,
        dispute_deadline: U64(1_793_577_600_000_000_000),
        arbiter_window_ns: U64(604_800_000_000_000),
        dispute,
    }
}

/// Una disputa abierta antes de `release_deadline`; si `resolved`, la cerro el arbitro dentro de su plazo.
fn disputa(opened_by: Party, resolved: bool) -> Dispute {
    Dispute {
        opened_by,
        reason_hash: hex64(0x22),
        opened_at: U64(1_793_500_000_000_000_000),
        resolved_at: resolved.then_some(U64(1_793_510_000_000_000_000)),
    }
}

/// La misma disputa cerrada por `resolve_expired_dispute`: justo al vencer el plazo del arbitro (`opened_at +
/// arbiter_window_ns`, el de [`real`]).
fn vencida(opened_by: Party) -> Dispute {
    let mut d = disputa(opened_by, true);
    d.resolved_at = Some(U64(1_793_500_000_000_000_000 + 604_800_000_000_000));
    d
}

fn bytes(p: &Payment) -> Vec<u8> {
    serde_json::to_vec(p).unwrap()
}

/// Lo que el espejo leyo es exactamente lo que el escrow tiene.
fn igual(e: &EscrowPayment, p: &Payment) {
    assert_eq!(e.operator_id, p.operator_id);
    assert_eq!(e.payer, p.payer);
    assert_eq!(e.receiver, p.receiver);
    assert_eq!(e.amount, p.amount.0);
    assert_eq!(e.released, p.released.0);
    assert_eq!(e.refunded, p.refunded.0);
    assert_eq!(e.fee_bps, p.fee_bps);
    assert_eq!(e.fee_recipient, p.fee_recipient);
    assert_eq!(e.fee_bps_max, p.fee_bps_max);
    assert_eq!(e.created_at, p.created_at.0);
    assert_eq!(e.release_deadline, p.release_deadline.0);
    assert_eq!(e.reclaim_after, p.reclaim_after.0);
    assert_eq!(e.terms_hash, p.terms_hash);
    assert_eq!(format!("{:?}", e.status), format!("{:?}", p.status));
    assert_eq!(e.storage_charged, p.storage_charged.0);
    assert_eq!(e.sponsored, p.sponsored);
    assert_eq!(e.dispute_deadline, p.dispute_deadline.0);
    assert_eq!(e.arbiter_window_ns, p.arbiter_window_ns.0);
    match (&e.dispute, &p.dispute) {
        (None, None) => {}
        (Some(a), Some(b)) => {
            assert_eq!(format!("{:?}", a.opened_by), format!("{:?}", b.opened_by));
            assert_eq!(a.reason_hash, b.reason_hash);
            assert_eq!(a.opened_at, b.opened_at.0);
            assert_eq!(a.resolved_at, b.resolved_at.map(|t| t.0));
        }
        _ => panic!("dispute distinta"),
    }
}

/// Un pago de 1.000.000 de PAYER a WORKER en `status`.
fn r(status: PaymentStatus, released: u128, refunded: u128, d: Option<Dispute>) -> Payment {
    real(PAYER, WORKER, status, 1_000_000, released, refunded, d)
}

/// Cada estado que el escrow puede escribir (`lifecycle.rs` y `dispute.rs`), con la respuesta que da el verificador.
#[test]
fn cada_estado_que_escribe_el_escrow() {
    use Party::*;
    use PaymentStatus::*;
    use VerifierError::{E13PagoSinLiberar as E13, E16DisputaVencida as E16};
    let abierta = |p| Some(disputa(p, false));
    let resuelta = |p| Some(disputa(p, true));
    let casos: Vec<(Payment, Result<u128, VerifierError>)> = vec![
        (r(Authorized, 0, 0, None), Err(E13)),
        (r(PartiallyReleased, 400_000, 0, None), Ok(400_000)),
        (r(Released, 1_000_000, 0, None), Ok(1_000_000)),
        (r(Refunded, 0, 1_000_000, None), Err(E13)),
        (r(Refunded, 400_000, 600_000, None), Ok(400_000)),
        (r(Reclaimed, 0, 1_000_000, None), Err(E13)),
        (r(Reclaimed, 250_000, 750_000, None), Ok(250_000)),
        (r(Disputed, 0, 0, abierta(Payer)), Err(E13)),
        (r(Disputed, 300_000, 0, abierta(Payer)), Ok(300_000)),
        (r(Disputed, 300_000, 0, abierta(Receiver)), Ok(300_000)),
        // El arbitro, en su plazo.
        (r(Resolved, 0, 1_000_000, resuelta(Payer)), Err(E13)),
        (
            r(Resolved, 500_000, 500_000, resuelta(Receiver)),
            Ok(500_000),
        ),
        (r(Resolved, 1_000_000, 0, resuelta(Payer)), Ok(1_000_000)),
        // Vencida: la del payer antes de release_deadline le da lo vivo al receiver (con o sin liberacion previa el
        // `Payment` queda igual: `released == amount`); la del receiver se lo devuelve al payer.
        (r(Resolved, 1_000_000, 0, Some(vencida(Payer))), Err(E16)),
        (
            r(Resolved, 300_000, 700_000, Some(vencida(Receiver))),
            Ok(300_000),
        ),
        (r(Resolved, 0, 1_000_000, Some(vencida(Receiver))), Err(E13)),
    ];
    for (p, esperado) in casos {
        let b = bytes(&p);
        let e: Option<EscrowPayment> = serde_json::from_slice(&b).unwrap();
        igual(&e.unwrap(), &p);
        for (client, role) in [(PAYER, "payer"), (WORKER, "receiver")] {
            let r = check_payment(&acc(client), Ok(b.clone()));
            match esperado {
                Ok(w) => {
                    let fp = r.unwrap();
                    assert_eq!(fp.weight, U128(w));
                    assert_eq!(fp.context, "em");
                    // `role` es el de la contraparte.
                    assert_ne!(fp.role, role);
                }
                Err(err) => assert_eq!(r, Err(err), "{p:?}"),
            }
        }
        assert_eq!(
            check_payment(&acc(OTHER), Ok(b)).err(),
            Some(esperado.map_or_else(|err| err, |_| VerifierError::E15NoEsParte))
        );
    }
}

/// `get_payment` devuelve `Option<Payment>`: `None` es `null`.
#[test]
fn none_del_escrow_es_e12() {
    let b = serde_json::to_vec(&Option::<Payment>::None).unwrap();
    assert_eq!(check_raw(PAYER, &b), Err(VerifierError::E12PagoInexistente));
}

fn cuenta() -> impl Strategy<Value = AccountId> {
    prop_oneof![
        Just(acc("agente.testnet")),
        Just(acc("worker.testnet")),
        Just(acc("a.near")),
        Just(acc(&"0123456789abcdef".repeat(4))),
        Just(acc("em-capturer-v2.arras-escrow.near")),
    ]
}

/// Un instante cerca de `base` (en el borde, un nanosegundo antes o despues) o cualquiera.
fn cerca(base: u64) -> impl Strategy<Value = u64> {
    prop_oneof![
        Just(base),
        Just(base.saturating_sub(1)),
        Just(base.saturating_add(1)),
        any::<u64>(),
    ]
}

/// `release_deadline`, `arbiter_window_ns` y la disputa (quien, `opened_at`, `resolved_at`) cerca de los dos bordes de la
/// vencida: `opened_at` frente a `release_deadline` y `resolved_at` frente a `opened_at + arbiter_window_ns`.
fn plazos() -> impl Strategy<Value = (u64, u64, u64, u64)> {
    (
        any::<u64>(),
        prop_oneof![Just(604_800_000_000_000u64), any::<u64>()],
    )
        .prop_flat_map(|(rd, ventana)| {
            cerca(rd).prop_flat_map(move |abierta| {
                cerca(abierta.saturating_add(ventana))
                    .prop_map(move |resuelta| (rd, ventana, abierta, resuelta))
            })
        })
}

fn estado_y_disputa() -> impl Strategy<Value = (PaymentStatus, Option<Dispute>)> {
    use PaymentStatus::*;
    let parte = prop_oneof![Just(Party::Payer), Just(Party::Receiver)];
    (0usize..7, parte, any::<u64>(), any::<u64>(), any::<u8>()).prop_map(
        |(i, quien, abierta, resuelta, h)| {
            let d = |resolved_at: Option<U64>| Dispute {
                opened_by: quien,
                reason_hash: hex64(h),
                opened_at: U64(abierta),
                resolved_at,
            };
            match i {
                0 => (Authorized, None),
                1 => (PartiallyReleased, None),
                2 => (Released, None),
                3 => (Refunded, None),
                4 => (Reclaimed, None),
                5 => (Disputed, Some(d(None))),
                _ => (Resolved, Some(d(Some(U64(resuelta))))),
            }
        },
    )
}

proptest! {
    // Sin `failure_persistence`: un contraejemplo no deja archivos en el arbol (los mutantes lo necesitan limpio).
    #![proptest_config(ProptestConfig {
        cases: 512,
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    /// Cualquier `Payment` coherente que el escrow serialice se lee entero y da lo que dice la decision.
    #[test]
    fn todo_payment_del_escrow_se_lee_entero(
        payer in cuenta(),
        receiver in cuenta(),
        client in cuenta(),
        amount in any::<u128>(),
        r1 in any::<u128>(),
        r2 in any::<u128>(),
        fee_bps in any::<u16>(),
        fee_bps_max in any::<u16>(),
        tiempos in any::<[u64; 5]>(),
        storage in any::<u128>(),
        sponsored in any::<bool>(),
        (status, dispute) in estado_y_disputa(),
        terms in any::<u8>(),
        (rd, ventana, abierta, resuelta) in plazos(),
        cerca_de_los_bordes in any::<bool>(),
    ) {
        // `x % (n + 1)` en `[0, n]`, sin desbordar con `n = u128::MAX`.
        let hasta = |x: u128, n: u128| n.checked_add(1).map_or(x, |m| x % m);
        let released = hasta(r1, amount);
        let refunded = hasta(r2, amount - released);
        // La mitad de las veces, los plazos y la disputa cerca de los bordes de la vencida.
        let (release_deadline, arbiter_window_ns, dispute) = if cerca_de_los_bordes {
            let d = dispute.map(|d| Dispute {
                opened_at: U64(abierta),
                resolved_at: d.resolved_at.map(|_| U64(resuelta)),
                ..d
            });
            (rd, ventana, d)
        } else {
            (tiempos[1], tiempos[4], dispute)
        };
        let p = Payment {
            operator_id: "em".into(),
            payer: payer.clone(),
            receiver: receiver.clone(),
            amount: U128(amount),
            released: U128(released),
            refunded: U128(refunded),
            fee_bps,
            fee_recipient: acc("em-fees.arras-dev.testnet"),
            fee_bps_max,
            created_at: U64(tiempos[0]),
            release_deadline: U64(release_deadline),
            reclaim_after: U64(tiempos[2]),
            terms_hash: hex64(terms),
            status,
            storage_charged: U128(storage),
            sponsored,
            dispute_deadline: U64(tiempos[3]),
            arbiter_window_ns: U64(arbiter_window_ns),
            dispute,
        };
        let b = bytes(&p);
        let e: Option<EscrowPayment> = serde_json::from_slice(&b).unwrap();
        igual(&e.unwrap(), &p);
        // `dispute.rs`: el arbitro solo cierra con `now < opened_at + arbiter_window_ns` (saturado) y la vencida le da
        // lo vivo al receiver si la abrio el payer antes de `release_deadline`.
        let vencida_al_receiver = p.dispute.as_ref().is_some_and(|d| {
            let plazo = d.opened_at.0.saturating_add(p.arbiter_window_ns.0);
            let arbitro_podia = d.resolved_at.is_some_and(|t| t.0 < plazo);
            let cobra_el_receiver =
                d.opened_by == Party::Payer && d.opened_at.0 < p.release_deadline.0;
            d.resolved_at.is_some() && !arbitro_podia && cobra_el_receiver
        });
        let esperado = if released == 0 {
            Err(VerifierError::E13PagoSinLiberar)
        } else if vencida_al_receiver {
            Err(VerifierError::E16DisputaVencida)
        } else if payer == receiver {
            Err(VerifierError::E14PayerEsReceiver)
        } else if client == payer {
            Ok((receiver, "receiver"))
        } else if client == receiver {
            Ok((payer, "payer"))
        } else {
            Err(VerifierError::E15NoEsParte)
        };
        let r = check_payment(&client, Ok(b));
        prop_assert_eq!(
            r,
            esperado.map(|(counterparty, role)| FeedbackProof {
                counterparty,
                role: role.into(),
                context: "em".into(),
                weight: U128(released),
            })
        );
    }
}
