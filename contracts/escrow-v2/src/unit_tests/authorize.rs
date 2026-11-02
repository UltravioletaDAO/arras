//! C4: `ft_on_transfer` con guardas, TVL y storage. U-03..U-08, U-17, U-18, U-25, U-26.

use super::common::*;
use crate::*;

const V1_ID: &str = "DN5eMA3sNES7xsoy6qvy24cVF6Dmwjxpkvd2b4e8P38M";

/// U-03 `authorize_ok`: predecessor = USDC testnet, `sender_id = payer-demo.testnet`, msg de V-1 -> `Value(0)`,
/// `payment_id` de V-1 y los tres TVL = 1000000.
#[test]
fn u03_authorize_ok() {
    let mut c = setup();
    let back = ft_on_transfer_as(
        &mut c,
        USDC,
        PAYER,
        1_000_000,
        &msg_v1().to_canonical_json(),
    );
    assert_eq!(back, 0);
    let p = c.get_payment(V1_ID.into()).expect("el pago de V-1 existe");
    assert_eq!(c.payments.len(), 1);
    assert_eq!(c.tvl_global, 1_000_000);
    assert_eq!(
        c.get_operator(OP.into()).unwrap().tvl_operador,
        U128(1_000_000)
    );
    assert_eq!(c.payer_tvl_of(OP, &acc(PAYER)), 1_000_000);
    assert_eq!(p.payer, acc(PAYER), "payer = sender_id, nunca el token");
    assert_eq!(p.receiver, acc(WORKER));
    assert_eq!(
        (p.amount, p.released, p.refunded),
        (U128(1_000_000), U128(0), U128(0))
    );
    assert_eq!(
        (p.fee_bps, p.fee_bps_max, p.fee_recipient.clone()),
        (1300, 1300, acc(FEES))
    );
    assert_eq!(
        (p.created_at, p.release_deadline, p.reclaim_after),
        (U64(NOW), U64(RELEASE), U64(RECLAIM))
    );
    assert_eq!(p.terms_hash, "0".repeat(64));
    assert_eq!(p.status, PaymentStatus::Authorized);
    assert!(!p.sponsored);
    assert_eq!(
        c.list_payments(OP.into(), 0, 50),
        vec![(V1_ID.to_string(), p)]
    );
    let l = logs();
    assert_eq!(l.len(), 1);
    assert_eq!(
        l[0],
        format!(
            r#"EVENT_JSON:{{"standard":"arras","version":"1.0.0","event":"authorized","data":[{{"payment_id":"{V1_ID}","operator_id":"em","token":"{USDC}","payer":"payer-demo.testnet","receiver":"worker-demo.testnet","amount":"1000000","fee_bps":1300,"fee_recipient":"em-fees.arras-dev.testnet","terms_hash":"{}"}}]}}"#,
            "0".repeat(64)
        )
    );
    // Las tres entradas de claimable quedan precargadas en 0.
    for a in [WORKER, FEES, PAYER] {
        assert_eq!(c.claimable.get(&acc(a)).copied(), Some(0), "{a}");
    }
}

/// U-04 `authorize_token_falso`: `E01`, estado intacto.
#[test]
fn u04_authorize_token_falso() {
    let mut c = setup();
    let m = msg_v1().to_canonical_json();
    for falso in [OTHER, PAYER, CONTRACT, "usdc.fakes.testnet"] {
        assert_rechazo(
            &mut c,
            falso,
            PAYER,
            1_000_000,
            &m,
            "E01_TOKEN_NO_PERMITIDO",
        );
    }
}

/// U-05 `authorize_tope_por_pago`: 10000000 -> OK; 10000001 -> `E06`.
#[test]
fn u05_authorize_tope_por_pago() {
    let mut c = setup();
    assert_rechazo(
        &mut c,
        USDC,
        PAYER,
        10_000_001,
        &msg_salt(1),
        "E06_TOPE_POR_PAGO",
    );
    assert_rechazo(
        &mut c,
        USDC,
        PAYER,
        u128::MAX,
        &msg_salt(1),
        "E06_TOPE_POR_PAGO",
    );
    authorize(&mut c, PAYER, 10_000_000, &msg_salt(1));
    assert_eq!(c.tvl_global, 10_000_000);
}

/// U-06 `authorize_campo_desconocido`: `E03` (y el resto de los `msg` invalidos).
#[test]
fn u06_authorize_campo_desconocido() {
    let mut c = setup();
    let m = msg_v1().to_canonical_json();
    for malo in [
        m.replace('}', ",\"extra\":true}"),
        m.replace("\"v\":1", "\"v\":2"),
        m.replace("authorize", "budget"),
        m.replace(&"0".repeat(64), &"F".repeat(64)),
        "{}".to_string(),
        String::new(),
    ] {
        assert_rechazo(&mut c, USDC, PAYER, 1_000_000, &malo, "E03_MSG_INVALIDO");
    }
}

/// U-07 `authorize_plazos`: `release_deadline >= reclaim_after` -> `E11`; `now >= release_deadline` -> `E11`.
#[test]
fn u07_authorize_plazos() {
    let mut c = setup();
    let mut m = msg_v1();
    m.reclaim_after = m.release_deadline;
    assert_rechazo(
        &mut c,
        USDC,
        PAYER,
        1_000_000,
        &m.to_canonical_json(),
        "E11_PLAZOS",
    );
    m.reclaim_after = m.release_deadline - 1;
    assert_rechazo(
        &mut c,
        USDC,
        PAYER,
        1_000_000,
        &m.to_canonical_json(),
        "E11_PLAZOS",
    );
    let mut m = msg_v1();
    m.release_deadline = NOW;
    assert_rechazo(
        &mut c,
        USDC,
        PAYER,
        1_000_000,
        &m.to_canonical_json(),
        "E11_PLAZOS",
    );
    m.release_deadline = NOW - 1;
    assert_rechazo(
        &mut c,
        USDC,
        PAYER,
        1_000_000,
        &m.to_canonical_json(),
        "E11_PLAZOS",
    );
    // El borde que pasa: now + 1 < reclaim_after.
    m.release_deadline = NOW + 1;
    m.reclaim_after = NOW + 2;
    authorize(&mut c, PAYER, 1_000_000, &m.to_canonical_json());
}

/// U-08 `authorize_duplicado`: el segundo -> `E13` (la parte de la lapida tras `compact` esta en C6, U-08b).
#[test]
fn u08_authorize_duplicado() {
    let mut c = setup();
    let m = msg_v1().to_canonical_json();
    authorize(&mut c, PAYER, 1_000_000, &m);
    assert_rechazo(&mut c, USDC, PAYER, 1_000_000, &m, "E13_ID_USADO");
    // Otro monto u otro payer es otro id: pasa.
    authorize(&mut c, PAYER, 1_000_001, &m);
}

/// U-17 `tope_global`: con topes de prueba, el byte que pasa `tope_tvl_global` -> `E10`.
#[test]
fn u17_tope_global() {
    let mut c = new_contract();
    c.tope_tvl_global = 3_000_000;
    ctx(OWNER, 1, NOW);
    c.register_operator(OP.into(), operator_input());
    ctx(PAYER, near(1).as_yoctonear(), NOW);
    c.storage_deposit(None, None);
    authorize(&mut c, PAYER, 2_000_000, &msg_salt(1));
    authorize(&mut c, PAYER, 999_999, &msg_salt(2));
    assert_rechazo(&mut c, USDC, PAYER, 10_001, &msg_salt(3), "E10_TVL_GLOBAL");
    // min_amount es 10000: para el byte exacto se usa un operador con minimo 1.
    ctx(OWNER, 1, NOW);
    let mut i = operator_input();
    i.min_amount = U128(1);
    c.register_operator("uno".into(), i);
    let mut uno = msg_v1();
    uno.operator_id = "uno".into();
    authorize(&mut c, PAYER, 1, &uno.to_canonical_json());
    assert_eq!(c.tvl_global, 3_000_000);
    let mut m = msg_v1();
    m.operator_id = "uno".into();
    m.salt[0] = 9;
    assert_rechazo(
        &mut c,
        USDC,
        PAYER,
        1,
        &m.to_canonical_json(),
        "E10_TVL_GLOBAL",
    );
}

/// U-18 `tope_operador`: `E09`.
#[test]
fn u18_tope_operador() {
    let mut c = new_contract();
    ctx(OWNER, 1, NOW);
    let mut i = operator_input();
    i.tope_tvl_operador = U128(3_000_000);
    i.min_amount = U128(1);
    c.register_operator(OP.into(), i);
    ctx(PAYER, near(1).as_yoctonear(), NOW);
    c.storage_deposit(None, None);
    authorize(&mut c, PAYER, 2_999_999, &msg_salt(1));
    assert_rechazo(&mut c, USDC, PAYER, 2, &msg_salt(2), "E09_TVL_OPERADOR");
    authorize(&mut c, PAYER, 1, &msg_salt(2));
    assert_rechazo(&mut c, USDC, PAYER, 1, &msg_salt(3), "E09_TVL_OPERADOR");
    assert_eq!(
        c.get_operator(OP.into()).unwrap().tvl_operador,
        U128(3_000_000)
    );
}

/// `E08` en el borde (W-07 lo prueba en el sandbox).
#[test]
fn tope_tvl_por_payer_borde() {
    let mut c = setup();
    for n in 0..2 {
        authorize(&mut c, PAYER, 10_000_000, &msg_salt(n));
    }
    assert_eq!(c.payer_tvl_of(OP, &acc(PAYER)), TOPE_TVL_POR_PAYER);
    assert_rechazo(
        &mut c,
        USDC,
        PAYER,
        MIN_AMOUNT,
        &msg_salt(9),
        "E08_TVL_PAYER",
    );
    // Otro payer sigue.
    ctx(OTHER, near(1).as_yoctonear(), NOW);
    c.storage_deposit(None, None);
    authorize(&mut c, OTHER, MIN_AMOUNT, &msg_salt(9));
}

/// U-25 `receiver_invalido`: receiver = contrato o = payer -> `E15`.
#[test]
fn u25_receiver_invalido() {
    let mut c = setup();
    for r in [CONTRACT, PAYER] {
        let mut m = msg_v1();
        m.receiver = acc(r);
        assert_rechazo(
            &mut c,
            USDC,
            PAYER,
            1_000_000,
            &m.to_canonical_json(),
            "E15_RECEIVER_INVALIDO",
        );
    }
}

/// `E04`, `E05` y `E12`.
#[test]
fn operador_minimo_y_fee() {
    let mut c = setup();
    let mut m = msg_v1();
    m.operator_id = "kk".into();
    assert_rechazo(
        &mut c,
        USDC,
        PAYER,
        1_000_000,
        &m.to_canonical_json(),
        "E04_OPERADOR_INEXISTENTE",
    );
    assert_rechazo(
        &mut c,
        USDC,
        PAYER,
        MIN_AMOUNT - 1,
        &msg_salt(1),
        "E05_DEBAJO_DEL_MINIMO",
    );
    authorize(&mut c, PAYER, MIN_AMOUNT, &msg_salt(1));
    let mut m = msg_v1();
    m.fee_bps_max = 1299;
    assert_rechazo(
        &mut c,
        USDC,
        PAYER,
        1_000_000,
        &m.to_canonical_json(),
        "E12_FEE_MAYOR_AL_MAXIMO",
    );
    m.fee_bps_max = 1300;
    authorize(&mut c, PAYER, 1_000_000, &m.to_canonical_json());
}

/// `E02`: la pausa bloquea authorize (U-11 completo, con `pause`, en C7).
#[test]
fn pausado_rechaza_authorize() {
    let mut c = setup();
    c.paused = true;
    assert_rechazo(
        &mut c,
        USDC,
        PAYER,
        1_000_000,
        &msg_v1().to_canonical_json(),
        "E02_PAUSADO",
    );
}

/// U-26 `storage_payer_y_patrocinio`: sin NEP-145 y sin patrocinio -> `E14`; con NEP-145 se descuenta del payer; con
/// patrocinio, hasta `sponsor_max_por_payer`.
#[test]
fn u26_storage_payer_y_patrocinio() {
    let mut c = setup();
    // Sin NEP-145 y sin patrocinio.
    assert_rechazo(
        &mut c,
        USDC,
        OTHER,
        1_000_000,
        &msg_salt(1),
        "E14_SIN_STORAGE",
    );

    // Con NEP-145: se descuenta exactamente lo que el pago escribe.
    let antes = c.storage.get(&acc(PAYER)).copied().unwrap();
    let id = authorize(&mut c, PAYER, 1_000_000, &msg_salt(1));
    let p = c.get_payment(id).unwrap();
    let despues = c.storage.get(&acc(PAYER)).copied().unwrap();
    assert!(!p.sponsored);
    assert_eq!(despues.available, antes.available - p.storage_charged.0);
    assert_eq!(despues.total, antes.total);
    assert_eq!(c.storage_available_total, despues.available);

    // Con patrocinio: el pool paga hasta sponsor_max_por_payer, despues E14; otro payer sigue.
    ctx(OWNER, near(1).as_yoctonear(), NOW);
    c.fund_operator_storage(OP.into());
    let mut usados = 0u128;
    let mut n = 10u8;
    loop {
        let m = msg_salt(n);
        let back = ft_on_transfer_as(&mut c, USDC, OTHER, MIN_AMOUNT, &m);
        if back != 0 {
            assert_eq!(logs(), vec!["arras:rechazo:E14_SIN_STORAGE".to_string()]);
            break;
        }
        let id = id_of(OTHER, MIN_AMOUNT, &m);
        let p = c.get_payment(id).unwrap();
        assert!(p.sponsored);
        usados += p.storage_charged.0;
        n += 1;
    }
    assert!(n > 10, "al menos un pago patrocinado");
    assert_eq!(c.sponsored_used_of(OP, &acc(OTHER)), usados);
    assert!(usados <= SPONSOR_MAX);
    let ultimo = c
        .get_payment(id_of(OTHER, MIN_AMOUNT, &msg_salt(n - 1)))
        .unwrap()
        .storage_charged
        .0;
    assert!(
        usados + ultimo > SPONSOR_MAX,
        "E14 llega cuando el siguiente no cabe"
    );
    assert_eq!(
        c.get_operator(OP.into()).unwrap().sponsor_pool.0,
        near(1).as_yoctonear() - usados
    );
    assert_rechazo(
        &mut c,
        USDC,
        OTHER,
        MIN_AMOUNT,
        &msg_salt(n),
        "E14_SIN_STORAGE",
    );
    // Otro payer sin NEP-145 todavia tiene su propio tope.
    let id = authorize(&mut c, "otro-payer.testnet", MIN_AMOUNT, &msg_salt(1));
    assert!(c.get_payment(id).unwrap().sponsored);
}

/// El costo cobrado es exactamente lo que el pago escribe mas la reserva de la disputa: `storage_charged == (bytes
/// nuevos + DISPUTE_RESERVE_BYTES) x 10^19`, medido con `env::storage_usage()` (primer pago con todas las entradas
/// nuevas, segundo con las entradas ya creadas, el `fee_recipient` igual al payer, y el camino patrocinado con su
/// entrada de `sponsored_used`).
#[test]
fn storage_charged_es_exacto() {
    let mut c = setup();
    let medir = |c: &mut Arras, sender: &str, msg: &str| -> (u64, u128) {
        ctx(USDC, 0, NOW);
        let antes = snapshot(c, sender).storage_usage;
        assert_eq!(
            call_ft_on_transfer(c, sender, 1_000_000, msg),
            0,
            "{:?}",
            logs()
        );
        let despues = snapshot(c, sender).storage_usage;
        (
            despues - antes,
            c.get_payment(id_of(sender, 1_000_000, msg))
                .unwrap()
                .storage_charged
                .0,
        )
    };
    let byte = env::storage_byte_cost().as_yoctonear();
    let (b1, c1) = medir(&mut c, PAYER, &msg_salt(11));
    assert_eq!(
        u128::from(b1 + DISPUTE_RESERVE_BYTES) * byte,
        c1,
        "primer pago"
    );
    let (b2, c2) = medir(&mut c, PAYER, &msg_salt(12));
    assert_eq!(
        u128::from(b2 + DISPUTE_RESERVE_BYTES) * byte,
        c2,
        "segundo pago"
    );
    assert!(b2 < b1);
    // fee_recipient == payer: una entrada de claimable menos.
    ctx(OWNER, 1, NOW);
    let mut i = operator_input();
    i.fee_recipient = acc("payer-dos.testnet");
    c.register_operator("kk".into(), i);
    ctx("payer-dos.testnet", near(1).as_yoctonear(), NOW);
    c.storage_deposit(None, None);
    let mut m = msg_v1();
    m.operator_id = "kk".into();
    let (b3, c3) = medir(&mut c, "payer-dos.testnet", &m.to_canonical_json());
    assert_eq!(
        u128::from(b3 + DISPUTE_RESERVE_BYTES) * byte,
        c3,
        "fee_recipient == payer"
    );
    // Patrocinado: suma la entrada de sponsored_used.
    ctx(OWNER, near(1).as_yoctonear(), NOW);
    c.fund_operator_storage(OP.into());
    let (b4, c4) = medir(&mut c, "sin-nep145.testnet", &msg_salt(4));
    assert_eq!(
        u128::from(b4 + DISPUTE_RESERVE_BYTES) * byte,
        c4,
        "patrocinado"
    );
    // Un id de 43 caracteres (V-2) tambien da exacto.
    let mut v2 = msg_v1();
    v2.salt[15] = 1;
    let id = id_of(PAYER, 1_000_000, &v2.to_canonical_json());
    assert_eq!(id.len(), 43);
    let (b5, c5) = medir(&mut c, PAYER, &v2.to_canonical_json());
    assert_eq!(
        u128::from(b5 + DISPUTE_RESERVE_BYTES) * byte,
        c5,
        "id de 43"
    );
}

/// La primera guarda que falla decide: para cada par de guardas consecutivas, una entrada que rompe las dos
/// devuelve la primera.
#[test]
fn orden_de_las_guardas() {
    let mut c = setup();
    let ok = msg_v1();
    let mut sin_op = ok.clone();
    sin_op.operator_id = "kk".into();
    let mut a_si_mismo = ok.clone();
    a_si_mismo.receiver = acc(PAYER);
    let mut plazos = ok.clone();
    plazos.release_deadline = NOW;
    let mut plazos_y_fee = plazos.clone();
    plazos_y_fee.fee_bps_max = 0;
    let malo = "{\"v\":9}".to_string();

    // E01 antes que E02.
    c.paused = true;
    assert_rechazo(
        &mut c,
        OTHER,
        PAYER,
        1_000_000,
        &malo,
        "E01_TOKEN_NO_PERMITIDO",
    );
    // E02 antes que E03.
    assert_rechazo(&mut c, USDC, PAYER, 1_000_000, &malo, "E02_PAUSADO");
    c.paused = false;
    // E03 antes que E04: un msg invalido nunca llega a buscar el operador.
    assert_rechazo(&mut c, USDC, PAYER, 1_000_000, &malo, "E03_MSG_INVALIDO");
    // E04 antes que E15.
    let mut m = sin_op.clone();
    m.receiver = acc(PAYER);
    assert_rechazo(
        &mut c,
        USDC,
        PAYER,
        1_000_000,
        &m.to_canonical_json(),
        "E04_OPERADOR_INEXISTENTE",
    );
    // E15 antes que E05.
    assert_rechazo(
        &mut c,
        USDC,
        PAYER,
        1,
        &a_si_mismo.to_canonical_json(),
        "E15_RECEIVER_INVALIDO",
    );
    // E05 antes que E06: con min_amount > tope_por_pago ambos fallan.
    c.tope_por_pago = 5_000;
    assert_rechazo(
        &mut c,
        USDC,
        PAYER,
        6_000,
        &ok.to_canonical_json(),
        "E05_DEBAJO_DEL_MINIMO",
    );
    c.tope_por_pago = TOPE_POR_PAGO;
    // E06 antes que E08: 10000001 tambien pasa el tope por payer si el payer ya tiene 15 USDC.
    authorize(&mut c, PAYER, 10_000_000, &msg_salt(1));
    authorize(&mut c, PAYER, 5_000_000, &msg_salt(2));
    assert_rechazo(
        &mut c,
        USDC,
        PAYER,
        10_000_001,
        &msg_salt(3),
        "E06_TOPE_POR_PAGO",
    );
    // E08 antes que E09 y E09 antes que E10.
    c.operators.get_mut(OP).unwrap().tope_tvl_operador = U128(15_000_000);
    c.tope_tvl_global = 15_000_000;
    assert_rechazo(
        &mut c,
        USDC,
        PAYER,
        5_000_001,
        &msg_salt(3),
        "E08_TVL_PAYER",
    );
    ctx(OTHER, near(1).as_yoctonear(), NOW);
    c.storage_deposit(None, None);
    assert_rechazo(
        &mut c,
        USDC,
        OTHER,
        1_000_000,
        &msg_salt(3),
        "E09_TVL_OPERADOR",
    );
    c.operators.get_mut(OP).unwrap().tope_tvl_operador = U128(TOPE_TVL_OPERADOR);
    // E10 antes que E11.
    assert_rechazo(
        &mut c,
        USDC,
        OTHER,
        1_000_000,
        &plazos.to_canonical_json(),
        "E10_TVL_GLOBAL",
    );
    c.tope_tvl_global = TOPE_TVL_GLOBAL;
    // E11 antes que E12.
    assert_rechazo(
        &mut c,
        USDC,
        OTHER,
        1_000_000,
        &plazos_y_fee.to_canonical_json(),
        "E11_PLAZOS",
    );
    // E12 antes que E13: el mismo msg ya autorizado, con la tarifa del operador subida por encima del maximo.
    let id = authorize(&mut c, OTHER, 1_000_000, &msg_salt(7));
    assert!(c.payments.contains_key(&id));
    c.operators.get_mut(OP).unwrap().fee_bps = 1301;
    assert_rechazo(
        &mut c,
        USDC,
        OTHER,
        1_000_000,
        &msg_salt(7),
        "E12_FEE_MAYOR_AL_MAXIMO",
    );
    c.operators.get_mut(OP).unwrap().fee_bps = 1300;
    // E13 antes que E14: el duplicado de un payer que ya no tiene saldo NEP-145 (y el pool esta vacio).
    let otro = c.storage.get(&acc(OTHER)).unwrap().available;
    c.storage.get_mut(&acc(OTHER)).unwrap().available = 0;
    c.storage_available_total -= otro;
    assert_rechazo(&mut c, USDC, OTHER, 1_000_000, &msg_salt(7), "E13_ID_USADO");
    assert_rechazo(
        &mut c,
        USDC,
        OTHER,
        1_000_000,
        &msg_salt(8),
        "E14_SIN_STORAGE",
    );
}

/// P3-4 de la refutacion: `list_payments` recorta `limit` a 50 y pagina por `from_index`.
#[test]
fn list_payments_pagina_con_tope_50() {
    let mut c = setup();
    ctx(PAYER, near(1).as_yoctonear(), NOW);
    c.storage_deposit(None, None);
    let mut ids = vec![];
    for n in 0..51u8 {
        ids.push(authorize(&mut c, PAYER, MIN_AMOUNT, &msg_salt(n)));
    }
    let pagina = |c: &Arras, from: u32, limit: u32| -> Vec<PaymentId> {
        c.list_payments(OP.into(), from, limit)
            .into_iter()
            .map(|(i, _)| i)
            .collect()
    };
    assert_eq!(pagina(&c, 0, 1000), ids[..50].to_vec());
    assert_eq!(pagina(&c, 50, 50), ids[50..].to_vec());
    assert_eq!(pagina(&c, 10, 3), ids[10..13].to_vec());
    assert!(pagina(&c, 51, 50).is_empty());
    assert!(pagina(&c, u32::MAX, 50).is_empty());
}

/// Ronda 1 de c0der (P3-1, mutante R8): con el saldo NEP-145 del payer **exactamente** igual al costo, paga el payer
/// (no el pool) y su `available` queda en 0.
#[test]
fn storage_payer_con_saldo_exacto_paga_el_payer() {
    let mut c = setup();
    ctx(OWNER, near(1).as_yoctonear(), NOW);
    c.fund_operator_storage(OP.into());
    let msg = msg_salt(1);
    ctx(USDC, 0, NOW);
    let costo = plan_cost(&c, PAYER, 1_000_000, &msg);
    let disponible = c.storage.get(&acc(PAYER)).unwrap().available;
    c.storage.get_mut(&acc(PAYER)).unwrap().available = costo;
    c.storage_available_total = c.storage_available_total - disponible + costo;
    let id = authorize(&mut c, PAYER, 1_000_000, &msg);
    let p = c.get_payment(id).unwrap();
    assert!(!p.sponsored, "saldo == costo: paga el payer");
    assert_eq!(p.storage_charged.0, costo);
    assert_eq!(c.storage.get(&acc(PAYER)).unwrap().available, 0);
    assert_eq!(
        c.get_operator(OP.into()).unwrap().sponsor_pool.0,
        near(1).as_yoctonear()
    );
}

/// Ronda 1 de c0der (P3-1, mutante R7): con el `sponsor_pool` **exactamente** igual al costo, paga el pool (no `E14`)
/// y queda en 0.
#[test]
fn storage_pool_exacto_patrocina() {
    let mut c = setup();
    ctx(OWNER, near(1).as_yoctonear(), NOW);
    c.fund_operator_storage(OP.into());
    let msg = msg_salt(1);
    ctx(USDC, 0, NOW);
    let costo = plan_cost(&c, OTHER, 1_000_000, &msg);
    c.operators.get_mut(OP).unwrap().sponsor_pool = U128(costo);
    let id = authorize(&mut c, OTHER, 1_000_000, &msg);
    let p = c.get_payment(id).unwrap();
    assert!(p.sponsored, "pool == costo: paga el pool");
    assert_eq!(p.storage_charged.0, costo);
    assert_eq!(c.get_operator(OP.into()).unwrap().sponsor_pool, U128(0));
}

/// El borde de al lado de R7: un yocto menos en el pool ya es `E14`, con el estado intacto.
#[test]
fn storage_pool_un_yocto_corto_es_e14() {
    let mut c = setup();
    ctx(OWNER, near(1).as_yoctonear(), NOW);
    c.fund_operator_storage(OP.into());
    let msg = msg_salt(1);
    ctx(USDC, 0, NOW);
    let costo = plan_cost(&c, OTHER, 1_000_000, &msg);
    c.operators.get_mut(OP).unwrap().sponsor_pool = U128(costo - 1);
    assert_rechazo(&mut c, USDC, OTHER, 1_000_000, &msg, "E14_SIN_STORAGE");
}
