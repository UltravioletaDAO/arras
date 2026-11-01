//! U-16 `invariante_proptest`: 10.000 secuencias de operaciones al azar sobre el contrato real (mock de near-sdk).
//! El retiro es **tres pasos intercalables**: `withdraw*` crea la promesa, el `ft_transfer` del token se ejecuta
//! (resultado al azar) y despues corre `on_withdraw`; entre cualquiera de ellos pueden pasar otras operaciones.
//!
//! Despues de cada paso:
//! - `sum(vivos) + sum(claimable) + in_flight == tvl_global` (sec. 3.5), con `tvl_operador` y `payer_tvl` iguales a
//!   lo vivo de cada uno e `in_flight` igual a la suma de los retiros pendientes;
//! - contra el saldo del token (modelado): `tvl_global <= ft_balance_of(contrato) + sum(transferidos sin callback)`.
//!   Es `<=` porque cualquiera puede mandar tokens directo al contrato (`Donate`); en la ventana entre un
//!   `ft_transfer` Ok y su callback la plata ya salio pero `in_flight` todavia la cuenta (P3-13 de la refutacion).
//! - toda llamada rechazada (panic) deja el estado igual.

use super::common::*;
use crate::*;
use near_sdk::PromiseError;
use proptest::prelude::*;

const DIA: u64 = 86_400 * 1_000_000_000;
const PAYERS: [&str; 3] = ["p0.testnet", "p1.testnet", "p2.testnet"];
const RECEIVERS: [&str; 2] = ["r0.testnet", "r1.testnet"];
const FEES2: &str = "kk-fees.testnet";
const CUENTAS: [&str; 7] = [
    "p0.testnet",
    "p1.testnet",
    "p2.testnet",
    "r0.testnet",
    "r1.testnet",
    FEES,
    FEES2,
];
const OPS: [&str; 2] = ["em", "kk"];

#[derive(Debug, Clone)]
enum Op {
    Authorize {
        payer: usize,
        receiver: usize,
        op: usize,
        amount: u128,
        salt: u16,
    },
    Release {
        pick: usize,
        amount: Option<u128>,
    },
    Refund {
        pick: usize,
        por_receiver: bool,
    },
    Reclaim {
        pick: usize,
    },
    Withdraw {
        cuenta: usize,
    },
    EjecutarTransfer {
        pick: usize,
        ok: bool,
    },
    Callback {
        pick: usize,
    },
    Donate {
        amount: u128,
    },
    Avanzar {
        medio_dias: u64,
    },
    Compact {
        pick: usize,
    },
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        4 => (0..3usize, 0..2usize, 0..2usize, 1..3_000_000u128, any::<u16>())
            .prop_map(|(payer, receiver, op, amount, salt)| Op::Authorize { payer, receiver, op, amount, salt }),
        3 => (any::<usize>(), prop::option::of(1..2_000_000u128)).prop_map(|(pick, amount)| Op::Release { pick, amount }),
        1 => (any::<usize>(), any::<bool>()).prop_map(|(pick, por_receiver)| Op::Refund { pick, por_receiver }),
        1 => any::<usize>().prop_map(|pick| Op::Reclaim { pick }),
        3 => (0..CUENTAS.len()).prop_map(|cuenta| Op::Withdraw { cuenta }),
        3 => (any::<usize>(), any::<bool>()).prop_map(|(pick, ok)| Op::EjecutarTransfer { pick, ok }),
        3 => any::<usize>().prop_map(|pick| Op::Callback { pick }),
        1 => (1..500_000u128).prop_map(|amount| Op::Donate { amount }),
        1 => (1..7u64).prop_map(|medio_dias| Op::Avanzar { medio_dias }),
        1 => any::<usize>().prop_map(|pick| Op::Compact { pick }),
    ]
}

struct Pendiente {
    cuenta: AccountId,
    amount: u128,
    /// `None`: el `ft_transfer` todavia no se ejecuto; `Some(ok)`: ya se ejecuto con ese resultado.
    ejecutado: Option<bool>,
}

struct Modelo {
    c: Arras,
    now: u64,
    saldo_token: u128,
    pendientes: Vec<Pendiente>,
    ids: Vec<PaymentId>,
}

/// Lo que una llamada rechazada no puede cambiar.
/// `(tvl_global, in_flight, pagos, sum(claimable), payer_tvl por (op, payer), (op, tvl_operador, sponsor_pool))`.
type Huella = (u128, u128, u32, u128, Vec<u128>, Vec<(String, u128, u128)>);

fn huella(c: &Arras) -> Huella {
    let claim: u128 = CUENTAS.iter().map(|a| c.claimable_of(&acc(a))).sum();
    let mut por_payer = vec![];
    for op in OPS {
        for p in PAYERS {
            por_payer.push(c.payer_tvl_of(op, &acc(p)));
        }
    }
    let ops = OPS
        .iter()
        .map(|o| {
            let op = c.operators.get(*o).unwrap();
            (o.to_string(), op.tvl_operador.0, op.sponsor_pool.0)
        })
        .collect();
    (
        c.tvl_global,
        c.in_flight,
        c.payments.len(),
        claim,
        por_payer,
        ops,
    )
}

fn intentar(m: &mut Modelo, f: impl FnOnce(&mut Arras)) -> bool {
    let antes = huella(&m.c);
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&mut m.c)));
    if r.is_err() {
        assert_eq!(huella(&m.c), antes, "un rechazo no cambia el estado");
    }
    r.is_ok()
}

impl Modelo {
    fn nuevo() -> Self {
        // Storage limpio por secuencia.
        let _ = near_sdk::mock::with_mocked_blockchain(|b| b.take_storage());
        let mut c = new_contract();
        for (op, fees) in [("em", FEES), ("kk", FEES2)] {
            ctx(OWNER, 1, NOW);
            let mut i = operator_input();
            i.fee_recipient = acc(fees);
            if op == "kk" {
                i.fee_bps = 1800;
                i.tope_tvl_operador = U128(8 * USDC_UNIT);
            }
            c.register_operator(op.into(), i);
        }
        ctx(OWNER, near(1).as_yoctonear(), NOW);
        c.fund_operator_storage("em".into());
        for p in &PAYERS[..2] {
            ctx(p, near(1).as_yoctonear(), NOW);
            c.storage_deposit(None, None);
        }
        Self {
            c,
            now: NOW,
            saldo_token: 0,
            pendientes: vec![],
            ids: vec![],
        }
    }

    fn pago(&self, pick: usize) -> Option<(PaymentId, Payment)> {
        if self.ids.is_empty() {
            return None;
        }
        let id = self.ids[pick % self.ids.len()].clone();
        self.c.payments.get(&id).cloned().map(|p| (id, p))
    }

    fn aplicar(&mut self, o: &Op) {
        let now = self.now;
        match o.clone() {
            Op::Authorize {
                payer,
                receiver,
                op,
                amount,
                salt,
            } => {
                let mut m = msg_v1();
                m.operator_id = OPS[op].into();
                m.receiver = acc(RECEIVERS[receiver]);
                m.release_deadline = now + DIA;
                m.reclaim_after = now + 2 * DIA;
                m.fee_bps_max = 1800;
                m.salt[..2].copy_from_slice(&salt.to_le_bytes());
                let msg = m.to_canonical_json();
                ctx(USDC, 0, now);
                let back = call_ft_on_transfer(&mut self.c, PAYERS[payer], amount, &msg);
                // El token se queda con lo usado (amount - devuelto).
                self.saldo_token += amount - back;
                if back == 0 {
                    self.ids.push(id_of(PAYERS[payer], amount, &msg));
                }
            }
            Op::Release { pick, amount } => {
                let Some((id, p)) = self.pago(pick) else {
                    return;
                };
                let capturer = self.c.operators.get(&p.operator_id).unwrap().capturers[0].clone();
                ctx(capturer.as_str(), 0, now);
                intentar(self, |c| c.release(id, amount.map(U128)));
            }
            Op::Refund { pick, por_receiver } => {
                let Some((id, p)) = self.pago(pick) else {
                    return;
                };
                let quien = if por_receiver {
                    p.receiver.to_string()
                } else {
                    CAPTURER.to_string()
                };
                ctx(&quien, 0, now);
                intentar(self, |c| c.refund(id));
            }
            Op::Reclaim { pick } => {
                let Some((id, p)) = self.pago(pick) else {
                    return;
                };
                ctx(p.payer.as_str(), 0, now);
                intentar(self, |c| c.reclaim(id));
            }
            Op::Withdraw { cuenta } => {
                let a = acc(CUENTAS[cuenta]);
                let monto = self.c.claimable_of(&a);
                ctx(OTHER, 0, now);
                let a2 = a.clone();
                if intentar(self, move |c| {
                    let _ = c.withdraw_for(a2);
                }) {
                    self.pendientes.push(Pendiente {
                        cuenta: a,
                        amount: monto,
                        ejecutado: None,
                    });
                } else {
                    assert_eq!(monto, 0, "withdraw solo falla sin saldo (E30)");
                }
            }
            Op::EjecutarTransfer { pick, ok } => {
                let sin_ejecutar: Vec<usize> = (0..self.pendientes.len())
                    .filter(|i| self.pendientes[*i].ejecutado.is_none())
                    .collect();
                if sin_ejecutar.is_empty() {
                    return;
                }
                let i = sin_ejecutar[pick % sin_ejecutar.len()];
                self.pendientes[i].ejecutado = Some(ok);
                if ok {
                    self.saldo_token -= self.pendientes[i].amount;
                }
            }
            Op::Callback { pick } => {
                let listos: Vec<usize> = (0..self.pendientes.len())
                    .filter(|i| self.pendientes[*i].ejecutado.is_some())
                    .collect();
                if listos.is_empty() {
                    return;
                }
                let p = self.pendientes.remove(listos[pick % listos.len()]);
                let r = if p.ejecutado == Some(true) {
                    Ok(())
                } else {
                    Err(PromiseError::Failed)
                };
                ctx(CONTRACT, 0, now);
                let devuelto = self.c.on_withdraw(p.cuenta, U128(p.amount), r).0;
                assert_eq!(
                    devuelto,
                    if p.ejecutado == Some(true) {
                        p.amount
                    } else {
                        0
                    }
                );
            }
            Op::Donate { amount } => self.saldo_token += amount,
            Op::Avanzar { medio_dias } => {
                self.now += medio_dias * DIA / 2 + medio_dias * medio_dias * DIA
            }
            Op::Compact { pick } => {
                let Some((id, _)) = self.pago(pick) else {
                    return;
                };
                ctx(OTHER, 0, now);
                intentar(self, |c| c.compact(id));
            }
        }
    }

    fn verificar(&self) {
        let c = &self.c;
        let vivos: u128 = c
            .payments
            .values()
            .filter(|p| p.status.is_live())
            .map(|p| p.live_amount())
            .sum();
        let claim: u128 = CUENTAS.iter().map(|a| c.claimable_of(&acc(a))).sum();
        assert_eq!(
            vivos + claim + c.in_flight,
            c.tvl_global,
            "sum(vivos) + sum(claimable) + in_flight == tvl_global"
        );
        assert_eq!(
            c.in_flight,
            self.pendientes.iter().map(|p| p.amount).sum::<u128>(),
            "in_flight"
        );
        for op in OPS {
            let o = c.operators.get(op).unwrap();
            let vivo_op: u128 = c
                .payments
                .values()
                .filter(|p| p.operator_id == op && p.status.is_live())
                .map(|p| p.live_amount())
                .sum();
            assert_eq!(o.tvl_operador.0, vivo_op, "tvl_operador {op}");
            assert!(o.tvl_operador.0 <= o.tope_tvl_operador.0);
            for payer in PAYERS {
                let vivo: u128 = c
                    .payments
                    .values()
                    .filter(|p| p.operator_id == op && p.payer == acc(payer) && p.status.is_live())
                    .map(|p| p.live_amount())
                    .sum();
                assert_eq!(
                    c.payer_tvl_of(op, &acc(payer)),
                    vivo,
                    "payer_tvl ({op}, {payer})"
                );
            }
        }
        assert!(c.tvl_global <= c.tope_tvl_global);
        let transferidos: u128 = self
            .pendientes
            .iter()
            .filter(|p| p.ejecutado == Some(true))
            .map(|p| p.amount)
            .sum();
        assert!(
            c.tvl_global <= self.saldo_token + transferidos,
            "tvl_global <= ft_balance_of(contrato) (+ en la ventana)"
        );
        if transferidos == 0 {
            assert!(
                c.tvl_global <= self.saldo_token,
                "tvl_global <= ft_balance_of(contrato)"
            );
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        // 10.000 por la sec. 5; `ARRAS_PROPTEST_CASES` lo baja para corridas rapidas (nunca en la evidencia del PR).
        cases: std::env::var("ARRAS_PROPTEST_CASES").ok().and_then(|v| v.parse().ok()).unwrap_or(10_000),
        max_shrink_iters: 2_000,
        ..ProptestConfig::default()
    })]

    /// U-16 `invariante_proptest`.
    #[test]
    fn u16_invariante_proptest(ops in prop::collection::vec(op(), 1..40)) {
        let mut m = Modelo::nuevo();
        m.verificar();
        for o in &ops {
            m.aplicar(o);
            m.verificar();
        }
        // Al final se drenan los pendientes (transfer al azar ya decidido o Ok) y la invariante sigue.
        while let Some(p) = m.pendientes.pop() {
            let ok = p.ejecutado.unwrap_or(true);
            if p.ejecutado.is_none() && ok {
                m.saldo_token -= p.amount;
            }
            ctx(CONTRACT, 0, m.now);
            m.c.on_withdraw(p.cuenta, U128(p.amount), if ok { Ok(()) } else { Err(PromiseError::Failed) });
            m.verificar();
        }
    }
}
