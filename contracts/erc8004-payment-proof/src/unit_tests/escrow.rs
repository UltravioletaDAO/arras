//! La disputa vencida contra el escrow de verdad: `arras_escrow_v2::Arras` corriendo en el mock de near-sdk (su propio
//! codigo, no un JSON escrito a mano). El escrow autoriza, libera, abre y cierra cada disputa en el nanosegundo que
//! elige el test, y `check_payment` lee lo que devuelve su `get_payment`. Es lo que el sandbox no puede (su reloj no se
//! elige): en `opened_at + arbiter_window_ns - 1` la cierra el arbitro y la vencida da `E64`; en `opened_at +
//! arbiter_window_ns` la vencida y el arbitro da `E65`.

use super::*;
use arras_escrow_core::{AuthorizeMsg, PaymentIdInput, compute_payment_id};
use arras_escrow_v2::{Arras, OperatorInput};
use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;
use near_sdk::PromiseOrValue;

const TOKEN: &str = "usdc.arras-dev.testnet";
const OWNER: &str = "owner.arras-dev.testnet";
const CAPTURER: &str = "em-capturer.arras-dev.testnet";
const ARBITER: &str = "em-arbiter.arras-dev.testnet";
const FEES: &str = "em-fees.arras-dev.testnet";
const USDC: u128 = 1_000_000;
/// 2026-11-01T00:00Z; `release_deadline` un dia despues (`RD`) y `reclaim_after` dos.
const NOW: u64 = RD - DAY;
const RECLAIM: u64 = RD + DAY;
/// El plazo del arbitro del operador, congelado en cada pago al autorizar.
const W: u64 = 3 * DAY;

/// El contexto del escrow: `predecessor`, deposito y hora.
fn en(predecessor: &str, deposit: u128, ts: u64) {
    testing_env!(
        VMContextBuilder::new()
            .current_account_id(acc(ESCROW))
            .predecessor_account_id(acc(predecessor))
            .signer_account_id(acc(predecessor))
            .attached_deposit(NearToken::from_yoctonear(deposit))
            .account_balance(NearToken::from_near(10))
            .block_timestamp(ts)
            .build()
    );
}

/// Un escrow nuevo con el operador `em` (capturer, arbitro con plazo `W`, sin ticket) y el payer registrado.
fn escrow() -> Arras {
    let _ = near_sdk::mock::with_mocked_blockchain(|b| b.take_storage());
    en(ESCROW, 0, NOW);
    let mut c = Arras::new(
        acc(OWNER),
        acc(OWNER),
        acc(TOKEN),
        U128(10 * USDC),
        U128(1_000 * USDC),
    );
    en(OWNER, 1, NOW);
    c.register_operator(
        "em".into(),
        OperatorInput {
            owner: acc(OWNER),
            capturers: vec![acc(CAPTURER)],
            fee_bps: 1300,
            fee_recipient: acc(FEES),
            min_amount: U128(10_000),
            tope_tvl_operador: U128(200 * USDC),
            tope_tvl_por_payer: U128(50 * USDC),
            sponsor_max_por_payer: U128(0),
            arbiter: acc(ARBITER),
            arbiter_window_ns: U64(W),
            ticket_pubkey: None,
        },
    );
    en(PAYER, NearToken::from_millinear(500).as_yoctonear(), NOW);
    let _ = c.storage_deposit(None, None);
    c
}

/// authorize de 1 USDC de PAYER a WORKER por `ft_on_transfer` (el `msg` de v1: `dispute_deadline` = `reclaim_after`);
/// devuelve el `payment_id`.
fn autorizar(c: &mut Arras) -> String {
    let m = AuthorizeMsg {
        operator_id: "em".into(),
        receiver: acc(WORKER),
        release_deadline: RD,
        reclaim_after: RECLAIM,
        fee_bps_max: 1300,
        terms_hash: [0; 32],
        salt: [7; 16],
    };
    en(TOKEN, 0, NOW);
    match c.ft_on_transfer(acc(PAYER), U128(USDC), m.to_canonical_json()) {
        PromiseOrValue::Value(v) => assert_eq!(v.0, 0, "authorize rechazado"),
        PromiseOrValue::Promise(_) => panic!("ft_on_transfer no devuelve promesas"),
    }
    compute_payment_id(&PaymentIdInput::from_msg(&m, TOKEN, PAYER, USDC)).1
}

/// Lo que `get_payment` del escrow devuelve, por el verificador.
fn verificar(c: &Arras, id: &str, client: &str) -> Result<FeedbackProof, VerifierError> {
    let b = serde_json::to_vec(&c.get_payment(id.to_string())).unwrap();
    check_payment(&acc(client), Ok(b))
}

/// El mensaje del panic de `f` (vacio si no entra en panico).
fn panico<R>(f: impl FnOnce() -> R) -> String {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f))
        .err()
        .and_then(|e| e.downcast::<String>().ok())
        .map(|m| *m)
        .unwrap_or_default()
}

/// Un pago con `previo` liberado por el capturer en `NOW` y la disputa que abre `quien` en `abierta`. Devuelve el
/// escrow, el id y lo vivo.
fn disputado(previo: Option<u128>, quien: &str, abierta: u64) -> (Arras, String, u128) {
    let mut c = escrow();
    let id = autorizar(&mut c);
    if let Some(x) = previo {
        en(CAPTURER, 0, NOW);
        c.release(id.clone(), Some(U128(x)));
    }
    en(quien, 0, abierta);
    c.open_dispute(id.clone(), RAZON.into());
    let p = c.get_payment(id.clone()).unwrap();
    assert_eq!(p.dispute.as_ref().unwrap().opened_at, U64(abierta));
    (c, id, USDC - previo.unwrap_or(0))
}

/// El borde exacto, con y sin liberacion previa: un nanosegundo antes de `opened_at + arbiter_window_ns` solo puede el
/// arbitro (vale); en el borde solo la vencida, que con la disputa del payer antes de `release_deadline` le da lo vivo
/// al receiver (`E16` para todos, tambien por lo liberado antes).
#[test]
fn el_borde_del_plazo_del_arbitro_en_el_escrow_de_verdad() {
    let abierta = NOW + DAY / 2;
    let vence = abierta + W;
    for previo in [None, Some(300_000)] {
        // vence - 1: la vencida no puede (E64); el arbitro le da lo vivo al receiver.
        let (mut c, id, vivo) = disputado(previo, PAYER, abierta);
        en(OTHER, 0, vence - 1);
        let e = panico(|| c.resolve_expired_dispute(id.clone()));
        assert!(e.contains("E64"), "{e}");
        en(ARBITER, 0, vence - 1);
        c.resolve_dispute(id.clone(), U128(vivo), U128(0));
        let p = c.get_payment(id.clone()).unwrap();
        assert_eq!(p.dispute.unwrap().resolved_at, Some(U64(vence - 1)));
        assert_eq!(
            verificar(&c, &id, PAYER).map(|f| f.weight),
            Ok(U128(USDC)),
            "{previo:?}"
        );
        assert_eq!(verificar(&c, &id, WORKER).map(|f| f.weight), Ok(U128(USDC)));
        // vence: el arbitro ya no puede (E65); la vencida le da lo vivo al receiver.
        let (mut c, id, _) = disputado(previo, PAYER, abierta);
        en(ARBITER, 0, vence);
        let e = panico(|| c.resolve_dispute(id.clone(), U128(0), U128(0)));
        assert!(e.contains("E65"), "{e}");
        en(OTHER, 0, vence);
        c.resolve_expired_dispute(id.clone());
        let p = c.get_payment(id.clone()).unwrap();
        assert_eq!(
            (p.released, p.refunded, p.dispute.unwrap().resolved_at),
            (U128(USDC), U128(0), Some(U64(vence)))
        );
        for client in [PAYER, WORKER, OTHER] {
            assert_eq!(
                verificar(&c, &id, client),
                Err(VerifierError::E16DisputaVencida),
                "{previo:?} {client}"
            );
        }
    }
}

/// El arbitro en su plazo, con cualquier reparto: vale por lo que llego al receiver (o `E13` si nada).
#[test]
fn el_arbitro_en_plazo_vale() {
    let abierta = NOW + DAY / 2;
    for (quien, previo, to_receiver, peso) in [
        (PAYER, Some(300_000), 200_000, Some(500_000)),
        (WORKER, Some(300_000), 0, Some(300_000)),
        (PAYER, None, 1, Some(1)),
        (PAYER, None, 0, None),
    ] {
        let (mut c, id, vivo) = disputado(previo, quien, abierta);
        en(ARBITER, 0, abierta + 1);
        c.resolve_dispute(id.clone(), U128(to_receiver), U128(vivo - to_receiver));
        let r = verificar(&c, &id, WORKER).map(|f| f.weight.0);
        match peso {
            Some(w) => assert_eq!(r, Ok(w), "{quien} {previo:?}"),
            None => assert_eq!(r, Err(VerifierError::E13PagoSinLiberar)),
        }
    }
}

/// Vencida a favor del payer (la abrio el receiver, o el payer en o despues de `release_deadline`): el plazo no le dio
/// nada al receiver; vale lo que el capturer libero antes, o es `E13`. Un nanosegundo antes de `release_deadline`, la
/// del payer todavia es del receiver: `E16`.
#[test]
fn la_vencida_a_favor_del_payer_vale_por_lo_liberado_antes() {
    for (quien, abierta, previo, esperado) in [
        (WORKER, NOW + DAY / 2, Some(300_000), Ok(300_000)),
        (
            WORKER,
            NOW + DAY / 2,
            None,
            Err(VerifierError::E13PagoSinLiberar),
        ),
        (PAYER, RD, Some(300_000), Ok(300_000)),
        (PAYER, RD, None, Err(VerifierError::E13PagoSinLiberar)),
        (
            PAYER,
            RD - 1,
            Some(300_000),
            Err(VerifierError::E16DisputaVencida),
        ),
    ] {
        let (mut c, id, _) = disputado(previo, quien, abierta);
        en(OTHER, 0, abierta + W);
        c.resolve_expired_dispute(id.clone());
        assert_eq!(
            verificar(&c, &id, WORKER).map(|f| f.weight.0),
            esperado,
            "{quien} {abierta} {previo:?}"
        );
    }
}
