//! C6: `withdraw` sin candado y `compact`. U-14, U-15, U-15b, U-19 y U-08b (la lapida).

use super::common::*;
use crate::*;
use near_sdk::PromiseResult;

const COMPACT_AT: u64 = RECLAIM + RETENTION_NS;
const DIA_NS: u64 = 86_400 * 1_000_000_000;

fn released_v1() -> (Arras, PaymentId) {
    let (mut c, id) = setup_v1();
    ctx(CAPTURER, 0, NOW);
    c.release(id.clone(), None);
    (c, id)
}

fn callback(c: &mut Arras, account: &str, amount: u128, r: PromiseResult) -> u128 {
    ctx_callback(r, NOW);
    c.on_withdraw(acc(account), U128(amount)).0
}

fn withdrawn_log(account: &str, amount: u128, ok: bool) -> String {
    format!(
        r#"EVENT_JSON:{{"standard":"arras","version":"1.0.0","event":"withdrawn","data":[{{"account":"{account}","token":"{USDC}","amount":"{amount}","ok":{ok}}}]}}"#
    )
}

/// U-14 `withdraw_pone_cero`: `claimable == 0` (entrada presente) e `in_flight == amount`.
#[test]
fn u14_withdraw_pone_cero() {
    let (mut c, _) = released_v1();
    ctx(WORKER, 0, NOW);
    let _ = c.withdraw();
    assert_eq!(
        c.claimable.get(&acc(WORKER)).copied(),
        Some(0),
        "la entrada no se borra"
    );
    assert_eq!(c.in_flight, 870_000);
    assert_eq!(
        c.tvl_global, 1_000_000,
        "tvl_global baja solo en on_withdraw Ok"
    );
    assert_invariante(&c, &TODAS);
    // withdraw_for de cualquiera para el fee_recipient.
    ctx(OTHER, 0, NOW);
    let _ = c.withdraw_for(acc(FEES));
    assert_eq!(c.claimable.get(&acc(FEES)).copied(), Some(0));
    assert_eq!(c.in_flight, 1_000_000);
    assert_invariante(&c, &TODAS);
    // Sin saldo: E30 (tambien para una cuenta que nunca tuvo entrada).
    expect_panic("ARRAS_E30", || c.withdraw_for(acc(FEES)));
    expect_panic("ARRAS_E30", || c.withdraw_for(acc("nadie.testnet")));
    ctx(WORKER, 0, NOW);
    expect_panic("ARRAS_E30", || c.withdraw());
}

/// U-15 `on_withdraw_falla`: `claimable` restaurado, `in_flight == 0`, `withdrawn ok:false`.
#[test]
fn u15_on_withdraw_falla() {
    let (mut c, _) = released_v1();
    ctx(WORKER, 0, NOW);
    let _ = c.withdraw();
    assert_eq!(callback(&mut c, WORKER, 870_000, PromiseResult::Failed), 0);
    assert_eq!(c.claimable_of(&acc(WORKER)), 870_000);
    assert_eq!(c.in_flight, 0);
    assert_eq!(c.tvl_global, 1_000_000);
    assert_eq!(logs(), vec![withdrawn_log(WORKER, 870_000, false)]);
    assert_invariante(&c, &TODAS);
}

/// U-15b `on_withdraw_falla_con_release_intercalado`: withdraw -> release(+x) -> `on_withdraw(Err)` ->
/// `claimable == amount + x`.
#[test]
fn u15b_on_withdraw_falla_con_release_intercalado() {
    let (mut c, _) = released_v1();
    let id2 = authorize(&mut c, PAYER, 1_000_000, &msg_salt(2));
    ctx(WORKER, 0, NOW);
    let _ = c.withdraw();
    ctx(CAPTURER, 0, NOW);
    c.release(id2, Some(U128(400_000)));
    assert_eq!(c.claimable_of(&acc(WORKER)), 348_000);
    callback(&mut c, WORKER, 870_000, PromiseResult::Failed);
    assert_eq!(c.claimable_of(&acc(WORKER)), 870_000 + 348_000);
    assert_eq!(c.in_flight, 0);
    assert_invariante(&c, &TODAS);
}

/// U-19 `tvl_baja_solo_al_salir`: release no baja `tvl_global`; `on_withdraw` Ok si; tras retirar todo,
/// `tvl_global == 0`.
#[test]
fn u19_tvl_baja_solo_al_salir() {
    let (mut c, _) = released_v1();
    assert_eq!(c.tvl_global, 1_000_000);
    ctx(WORKER, 0, NOW);
    let _ = c.withdraw();
    assert_eq!(
        callback(&mut c, WORKER, 870_000, PromiseResult::Successful(vec![])),
        870_000
    );
    assert_eq!(logs(), vec![withdrawn_log(WORKER, 870_000, true)]);
    assert_eq!((c.tvl_global, c.in_flight), (130_000, 0));
    assert_invariante(&c, &TODAS);
    ctx(CAPTURER, 0, NOW);
    let _ = c.withdraw_for(acc(FEES));
    // Un exito con datos (aunque no sean JSON) cuenta como Ok y no entra en panico: el transfer se hizo.
    assert_eq!(
        callback(
            &mut c,
            FEES,
            130_000,
            PromiseResult::Successful(b"\x00basura no json".to_vec())
        ),
        130_000
    );
    assert_eq!(
        (c.tvl_global, c.in_flight, c.claimable_of(&acc(FEES))),
        (0, 0, 0)
    );
    assert_invariante(&c, &TODAS);
}

/// W-10 a nivel unitario: release -> withdraw OK -> release -> withdraw OK: el segundo cobra (sin candado).
#[test]
fn withdraw_dos_veces_sin_candado() {
    let (mut c, _) = released_v1();
    ctx(WORKER, 0, NOW);
    let _ = c.withdraw();
    callback(&mut c, WORKER, 870_000, PromiseResult::Successful(vec![]));
    let id2 = authorize(&mut c, PAYER, 1_000_000, &msg_salt(2));
    ctx(CAPTURER, 0, NOW);
    c.release(id2, None);
    ctx(WORKER, 0, NOW);
    let _ = c.withdraw();
    assert_eq!(c.in_flight, 870_000);
    callback(&mut c, WORKER, 870_000, PromiseResult::Successful(vec![]));
    assert_eq!(c.claimable_of(&acc(WORKER)), 0);
    assert_invariante(&c, &TODAS);
}

/// Dos retiros en vuelo de la misma cuenta: el segundo retira lo nuevo; con fallas en cualquier orden, nada se
/// pierde ni se duplica.
#[test]
fn dos_withdraw_en_vuelo() {
    let (mut c, _) = released_v1();
    let id2 = authorize(&mut c, PAYER, 1_000_000, &msg_salt(2));
    ctx(WORKER, 0, NOW);
    let _ = c.withdraw(); // 870000 en vuelo
    expect_panic("ARRAS_E30", || c.withdraw());
    ctx(CAPTURER, 0, NOW);
    c.release(id2, None);
    ctx(WORKER, 0, NOW);
    let _ = c.withdraw(); // otros 870000 en vuelo
    assert_eq!(c.in_flight, 1_740_000);
    callback(&mut c, WORKER, 870_000, PromiseResult::Failed);
    callback(&mut c, WORKER, 870_000, PromiseResult::Successful(vec![]));
    assert_eq!((c.claimable_of(&acc(WORKER)), c.in_flight), (870_000, 0));
    assert_eq!(c.tvl_global, 2_000_000 - 870_000);
    assert_invariante(&c, &TODAS);
}

/// `compact`: E20 -> E22 (no terminal) -> E28 (retencion); borra, guarda la lapida y devuelve al payer lo que el
/// borrado libera de verdad.
#[test]
fn compact_guardas_y_devolucion_exacta() {
    let (mut c, id) = released_v1();
    let live = authorize(&mut c, PAYER, 1_000_000, &msg_salt(2));
    ctx(OTHER, 0, COMPACT_AT);
    expect_panic("ARRAS_E20", || {
        c.compact("Sc4D4Jn3ErWE68FckdDTLzThk1FTR1ZfLzuBUE4EeRm".into())
    });
    expect_panic("ARRAS_E22: estado invalido para compact", || {
        c.compact(live.clone())
    });
    ctx(OTHER, 0, COMPACT_AT - 1);
    expect_panic("ARRAS_E28", || c.compact(id.clone()));
    expect_panic("ARRAS_E22", || c.compact(live.clone()));

    ctx(OTHER, 0, COMPACT_AT);
    let charged = c.get_payment(id.clone()).unwrap().storage_charged.0;
    let antes = snapshot(&mut c, PAYER);
    c.compact(id.clone());
    let despues = snapshot(&mut c, PAYER);
    let liberados = antes.storage_usage - despues.storage_usage;
    let devuelto = u128::from(liberados) * env::storage_byte_cost().as_yoctonear();
    assert!(
        devuelto > 0 && devuelto < charged,
        "devuelve lo liberado, no lo que queda (claimable, indice, payer_tvl)"
    );
    assert_eq!(
        despues.storage_payer.unwrap().1,
        antes.storage_payer.unwrap().1 + devuelto
    );
    assert_eq!(
        despues.storage_available_total,
        antes.storage_available_total + devuelto
    );
    assert_eq!(c.get_payment(id.clone()), None);
    assert!(
        c.used_ids
            .contains(&arras_escrow_core::decode_payment_id(&id).unwrap())
    );
    assert_eq!(
        logs(),
        vec![format!(
            r#"EVENT_JSON:{{"standard":"arras","version":"1.0.0","event":"compacted","data":[{{"payment_id":"{id}"}}]}}"#
        )]
    );
    // list_payments salta el compactado; compact otra vez -> E20.
    let lista: Vec<PaymentId> = c
        .list_payments(OP.into(), 0, 50)
        .into_iter()
        .map(|(i, _)| i)
        .collect();
    assert_eq!(lista, vec![live]);
    expect_panic("ARRAS_E20", || c.compact(id.clone()));
    // El dinero no se toca: claimable sigue.
    assert_eq!(c.claimable_of(&acc(WORKER)), 870_000);
    assert_invariante(&c, &TODAS);
}

/// U-08b (segunda parte de U-08): despues de `compact`, el mismo `msg` -> `E13` (lapida).
#[test]
fn u08b_lapida_tras_compact() {
    let (mut c, id) = setup_v1();
    ctx(CAPTURER, 0, NOW);
    c.refund(id.clone());
    ctx(OTHER, 0, COMPACT_AT);
    c.compact(id);
    assert_rechazo(
        &mut c,
        USDC,
        PAYER,
        1_000_000,
        &msg_v1().to_canonical_json(),
        "E13_ID_USADO",
    );
}

/// Un pago patrocinado devuelve al `sponsor_pool` y libera su cupo de `sponsored_used`.
#[test]
fn compact_patrocinado_devuelve_al_pool() {
    let mut c = setup();
    ctx(OWNER, near(1).as_yoctonear(), NOW);
    c.fund_operator_storage(OP.into());
    let id = authorize(&mut c, OTHER, 1_000_000, &msg_salt(1));
    assert!(c.get_payment(id.clone()).unwrap().sponsored);
    ctx(CAPTURER, 0, NOW);
    c.refund(id.clone());
    ctx(OTHER, 0, COMPACT_AT);
    let pool = c.get_operator(OP.into()).unwrap().sponsor_pool.0;
    let usado = c.sponsored_used_of(OP, &acc(OTHER));
    let antes = snapshot(&mut c, OTHER);
    c.compact(id);
    let despues = snapshot(&mut c, OTHER);
    let devuelto = u128::from(antes.storage_usage - despues.storage_usage)
        * env::storage_byte_cost().as_yoctonear();
    assert_eq!(
        c.get_operator(OP.into()).unwrap().sponsor_pool.0,
        pool + devuelto
    );
    assert_eq!(c.sponsored_used_of(OP, &acc(OTHER)), usado - devuelto);
    assert_eq!(despues.storage_payer, None);
}

/// P2-1 de la refutacion: un `payment_id` de 42 caracteres (salt `00..0415` sobre V-1) se autoriza y tambien se
/// compacta.
#[test]
fn compact_con_payment_id_de_42_caracteres() {
    let mut c = setup();
    let mut m = msg_v1();
    m.salt[14] = 0x04;
    m.salt[15] = 0x15;
    let id = authorize(&mut c, PAYER, 1_000_000, &m.to_canonical_json());
    assert_eq!(id, "1y1ZbrieUbaioiUBHxX7EyZMNkCis2tLKCs5Xxezgy");
    ctx(CAPTURER, 0, NOW);
    c.refund(id.clone());
    ctx(OTHER, 0, COMPACT_AT);
    c.compact(id.clone());
    assert_eq!(c.get_payment(id), None);
    assert_rechazo(
        &mut c,
        USDC,
        PAYER,
        1_000_000,
        &m.to_canonical_json(),
        "E13_ID_USADO",
    );
}

/// P3-2 de la refutacion: un exito del `ft_transfer` con cualquier dato (JSON o no) es Ok y no entra en panico.
#[test]
fn on_withdraw_no_deserializa_lo_que_devuelve_el_token() {
    for datos in [
        b"null".to_vec(),
        b"\"0\"".to_vec(),
        vec![0xff, 0x00, 0x13],
        vec![],
    ] {
        let _ = near_sdk::mock::with_mocked_blockchain(|b| b.take_storage());
        let (mut c, _) = released_v1();
        ctx(WORKER, 0, NOW);
        let _ = c.withdraw();
        assert_eq!(
            callback(
                &mut c,
                WORKER,
                870_000,
                PromiseResult::Successful(datos.clone())
            ),
            870_000,
            "{datos:?}"
        );
        assert_eq!(
            (c.claimable_of(&acc(WORKER)), c.in_flight, c.tvl_global),
            (0, 0, 130_000)
        );
    }
}

/// Ronda 1 de c0der (P2-1): el tope global cuenta lo ya liberado, reembolsado o recuperado que todavia no se retiro
/// (`claimable` e `in_flight`, sec. 3.5). `refund` y `reclaim` liberan el tope por payer y el del operador, no el
/// global: una sola cuenta lo puede mantener lleno hasta que ese saldo se retira. Un retiro fallido no lo libera; uno
/// exitoso si, y los pagos nuevos vuelven a pasar.
#[test]
fn tope_global_cuenta_lo_no_retirado_hasta_el_retiro() {
    let mut c = setup();
    ctx(PAYER, near(5).as_yoctonear(), NOW);
    c.storage_deposit(None, None);
    let mut t = NOW;
    let mut pagos = 0u32;
    loop {
        let mut m = msg_v1();
        m.release_deadline = t + 1;
        m.reclaim_after = t + 2;
        m.salt[..4].copy_from_slice(&pagos.to_le_bytes());
        let msg = m.to_canonical_json();
        ctx(USDC, 0, t);
        if call_ft_on_transfer(&mut c, PAYER, TOPE_POR_PAGO, &msg) != 0 {
            assert_eq!(logs(), vec!["arras:rechazo:E10_TVL_GLOBAL".to_string()]);
            break;
        }
        ctx(PAYER, 0, t + 2);
        c.reclaim(id_of(PAYER, TOPE_POR_PAGO, &msg));
        t += 3;
        pagos += 1;
        assert!(pagos <= 100, "el tope global corta en 1.000 USDC");
    }
    // Un payer, un operador: los topes por payer (20 USDC) y por operador (200) quedan libres; el global, lleno.
    assert_eq!(pagos, 100);
    assert_eq!(c.tvl_global, TOPE_TVL_GLOBAL);
    assert_eq!(c.payer_tvl_of(OP, &acc(PAYER)), 0);
    assert_eq!(c.get_operator(OP.into()).unwrap().tvl_operador, U128(0));
    assert_eq!(c.claimable_of(&acc(PAYER)), TOPE_TVL_GLOBAL);
    assert_rechazo(
        &mut c,
        USDC,
        OTHER,
        MIN_AMOUNT,
        &msg_salt(200),
        "E10_TVL_GLOBAL",
    );
    assert_invariante(&c, &TODAS);
    // Un retiro que falla (p. ej. el titular no puede recibir USDC) no libera nada.
    ctx(OTHER, 0, t);
    let _ = c.withdraw_for(acc(PAYER));
    on_withdraw_con(&mut c, PAYER, TOPE_TVL_GLOBAL, false, t);
    assert_eq!(
        (c.tvl_global, c.claimable_of(&acc(PAYER))),
        (TOPE_TVL_GLOBAL, TOPE_TVL_GLOBAL)
    );
    assert_rechazo(
        &mut c,
        USDC,
        OTHER,
        MIN_AMOUNT,
        &msg_salt(201),
        "E10_TVL_GLOBAL",
    );
    // `withdraw_for` lo puede disparar cualquiera: con el retiro exitoso el tope se libera.
    ctx(OTHER, 0, t);
    let _ = c.withdraw_for(acc(PAYER));
    on_withdraw_con(&mut c, PAYER, TOPE_TVL_GLOBAL, true, t);
    assert_eq!((c.tvl_global, c.claimable_of(&acc(PAYER))), (0, 0));
    ctx(OTHER, near(1).as_yoctonear(), t);
    c.storage_deposit(None, None);
    let mut m = msg_v1();
    m.release_deadline = t + DIA_NS;
    m.reclaim_after = t + 2 * DIA_NS;
    ctx(USDC, 0, t);
    assert_eq!(
        call_ft_on_transfer(&mut c, OTHER, MIN_AMOUNT, &m.to_canonical_json()),
        0,
        "{:?}",
        logs()
    );
    assert_invariante(&c, &TODAS);
}
