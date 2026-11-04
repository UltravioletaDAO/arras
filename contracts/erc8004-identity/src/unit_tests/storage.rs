//! U-ST: el storage. `REGISTER_MAX_BYTES` es exactamente el peor caso medido; la reserva cubre la unica cosa que una
//! transferencia puede hacer crecer (un contador por dueno nuevo); y la invariante: el storage del contrato nunca pasa
//! de lo que se cobro.

use super::common::*;
use crate::registry::MetadataEntry;
use crate::*;
use near_sdk::json_types::Base64VecU8;
use near_sdk::test_utils::get_logs;

fn al_tope(owner: &str) -> (String, Vec<MetadataEntry>) {
    let _ = owner;
    let uri = format!("https://{}", "a".repeat(MAX_AGENT_URI_LEN - 8));
    let entries = (0..MAX_REGISTER_METADATA)
        .map(|i| MetadataEntry {
            metadata_key: format!(
                "{i}{}",
                "k".repeat(erc8004_common::validate::MAX_METADATA_KEY_LEN - 1)
            ),
            metadata_value: Base64VecU8(vec![0xff; MAX_METADATA_VALUE_LEN]),
        })
        .collect();
    (uri, entries)
}

#[test]
fn ust1_peor_caso_de_register_es_register_max_bytes_y_entra_en_los_logs() {
    let owner = "o".repeat(64);
    let mut c = new_contract();
    let (uri, entries) = al_tope(&owner);
    let dep = 2 * NEAR;
    ctx(&owner, dep);
    c.flush_all();
    let antes = env::storage_usage();
    c.register(Some(uri), Some(entries));
    c.flush_all();
    let bytes = env::storage_usage() - antes;
    assert_eq!(bytes + TOKEN_RESERVE_BYTES, REGISTER_MAX_BYTES);
    assert_eq!(REGISTER_MAX_BYTES, 13_279);
    assert_eq!(
        transfers(),
        vec![(owner.clone(), dep - REGISTER_MAX_BYTES as u128 * BYTE)]
    );
    // NEAR corta en 16.384 bytes de logs por recibo: el peor caso de register entra.
    let total: usize = get_logs().iter().map(String::len).sum();
    assert!(total < 16_384, "logs: {total}");
    assert_eq!(get_logs().len(), 3 + MAX_REGISTER_METADATA);
}

#[test]
fn ust2_la_reserva_es_un_contador_por_dueno() {
    let mut c = new_contract();
    // El primer agente de un dueno crea su contador; el segundo no.
    ctx(ALICE, NEAR);
    c.flush_all();
    let a = env::storage_usage();
    c.register(None, None);
    c.flush_all();
    let primero = env::storage_usage() - a;
    ctx(ALICE, NEAR);
    c.flush_all();
    let a = env::storage_usage();
    c.register(None, None);
    c.flush_all();
    let segundo = env::storage_usage() - a;
    assert_eq!(primero - segundo, OWNER_COUNT_RECORD_BYTES);
    assert_eq!(TOKEN_RESERVE_BYTES, OWNER_COUNT_RECORD_BYTES);
    assert_eq!(OWNER_COUNT_RECORD_BYTES, 77);
}

#[test]
fn ust3_una_transferencia_solo_mueve_el_contador() {
    let largo = "l".repeat(64);
    let mut c = new_contract();
    reg(&mut c, ALICE);
    reg(&mut c, ALICE);
    let medir = |c: &mut IdentityRegistry, from: &str, to: &str, id: &str| -> i64 {
        ctx(from, 1);
        c.flush_all();
        let a = env::storage_usage() as i64;
        c.nft_transfer(acc(to), id.into(), None, None);
        c.flush_all();
        env::storage_usage() as i64 - a
    };
    // ALICE se queda con uno y la cuenta de 64 es nueva: +1 contador.
    assert_eq!(
        medir(&mut c, ALICE, &largo, "0"),
        OWNER_COUNT_RECORD_BYTES as i64
    );
    // ALICE se queda sin ninguno y la de 64 ya tenia: -1 contador.
    assert_eq!(
        medir(&mut c, ALICE, &largo, "1"),
        -(OWNER_COUNT_RECORD_BYTES as i64)
    );
    // De la de 64 a una corta nueva, quedandole uno: +1. El largo de las cuentas no cambia nada.
    assert_eq!(
        medir(&mut c, &largo, BOB, "0"),
        OWNER_COUNT_RECORD_BYTES as i64
    );
    assert_eq!(transfers(), vec![]);
}

/// La invariante de la seccion 2.4: en cualquier secuencia, lo que el contrato ocupa por encima de lo que ocupaba
/// recien desplegado nunca pasa de lo que cobro (lo cobrado menos lo devuelto).
#[test]
fn ust4_el_storage_nunca_pasa_de_lo_cobrado() {
    let mut c = new_contract();
    c.flush_all();
    let base = env::storage_usage() as i128;
    let mut neto: i128 = 0; // bytes cobrados - bytes devueltos
    let cuentas: Vec<String> = (0..12)
        .map(|i| {
            if i % 3 == 0 {
                format!("{}{i:02}", "x".repeat(62))
            } else {
                format!("c{i}.testnet")
            }
        })
        .collect();
    let mut duenos: Vec<usize> = vec![];
    let mut semilla: u64 = 0x5eed;
    let mut rnd = |n: usize| {
        semilla = semilla
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((semilla >> 33) as usize) % n
    };
    for paso in 0..400 {
        let op = if duenos.is_empty() { 0 } else { rnd(7) };
        match op {
            0 => {
                let w = rnd(cuentas.len());
                ctx(&cuentas[w], NEAR);
                c.register(Some(format!("https://{}", "u".repeat(rnd(40)))), None);
                neto += ((NEAR - refunded_to(&cuentas[w])) / BYTE) as i128;
                duenos.push(w);
            }
            1 | 2 => {
                let id = rnd(duenos.len());
                let to = rnd(cuentas.len());
                if to != duenos[id] {
                    ctx(&cuentas[duenos[id]], 1);
                    c.nft_transfer(acc(&cuentas[to]), id.to_string(), None, None);
                    neto -= (refunded_to(&cuentas[duenos[id]]) / BYTE) as i128;
                    duenos[id] = to;
                }
            }
            3 => {
                let id = rnd(duenos.len());
                let w = rnd(cuentas.len());
                ctx(&cuentas[duenos[id]], NEAR);
                if c.nft_approve(id.to_string(), acc(&cuentas[w]), None)
                    .is_none()
                {
                    neto += ((NEAR - refunded_to(&cuentas[duenos[id]])) / BYTE) as i128;
                }
            }
            4 => {
                let id = rnd(duenos.len());
                let w = rnd(cuentas.len());
                ctx(&cuentas[duenos[id]], 1);
                let me = cuentas[duenos[id]].clone();
                c.set_agent_wallet(id.to_string(), acc(&cuentas[w]), NOW.to_string());
                if cuentas[w] != me {
                    ctx(&cuentas[w], 1);
                    c.accept_agent_wallet(id.to_string());
                }
            }
            5 => {
                let id = rnd(duenos.len());
                ctx(&cuentas[duenos[id]], NEAR);
                c.set_agent_uri(
                    id.to_string(),
                    if rnd(2) == 0 {
                        String::new()
                    } else {
                        format!("a:{}", "v".repeat(rnd(60)))
                    },
                );
                neto += ((NEAR as i128) - refunded_to(&cuentas[duenos[id]]) as i128) / BYTE as i128;
            }
            _ => {
                let id = rnd(duenos.len());
                ctx(&cuentas[duenos[id]], 1);
                c.nft_revoke_all(id.to_string());
                neto -= (refunded_to(&cuentas[duenos[id]]) / BYTE) as i128;
            }
        }
        c.flush_all();
        let usado = env::storage_usage() as i128 - base;
        assert!(
            usado <= neto,
            "paso {paso}: usa {usado} bytes y se cobraron {neto}"
        );
    }
    assert!(neto > 0);
}

/// U-ST5 (refutador, O1): quedarse sin agentes no le devuelve a nadie el contador del dueno (lo que se libera es la
/// reserva que pago `register`, y la va a necesitar el proximo dueno nuevo). ALICE (1) -> BOB (1): nada vuelve a
/// ALICE; despues el ciclo BOB (2) -> cuenta sin agentes -> BOB, varias veces, con la cota de U-ST4 en cada paso.
#[test]
fn ust5_quedarse_sin_agentes_no_devuelve_el_contador() {
    let mut c = new_contract();
    c.flush_all();
    let base = env::storage_usage() as i128;
    let mut neto: i128 = 0;
    let cota = |c: &mut IdentityRegistry, neto: i128, paso: &str| {
        c.flush_all();
        let usado = env::storage_usage() as i128 - base;
        assert!(
            usado <= neto,
            "{paso}: usa {usado} bytes y se cobraron {neto}"
        );
    };
    for who in [ALICE, BOB] {
        ctx(who, NEAR);
        c.register(None, None);
        neto += ((NEAR - refunded_to(who)) / BYTE) as i128;
        cota(&mut c, neto, "register");
    }
    transfer(&mut c, ALICE, BOB, "0");
    assert_eq!(refunded_to(ALICE), 0);
    assert_eq!(transfers(), vec![]);
    cota(&mut c, neto, "ALICE -> BOB");
    for i in 0..6 {
        let nueva = format!("n{i}.testnet");
        transfer(&mut c, BOB, &nueva, "0");
        assert_eq!(transfers(), vec![]);
        cota(&mut c, neto, &format!("ciclo {i}: BOB -> {nueva}"));
        transfer(&mut c, &nueva, BOB, "0");
        assert_eq!(transfers(), vec![]);
        cota(&mut c, neto, &format!("ciclo {i}: {nueva} -> BOB"));
    }
    assert_eq!(c.nft_supply_for_owner(acc(BOB)).0, 2);
}
