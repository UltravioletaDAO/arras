//! U-16 de v2 (plan D8): `invariante_proptest` con presupuestos y disputas. 10.000 secuencias de operaciones al azar
//! sobre el contrato real (mock de near-sdk). El retiro es **tres pasos intercalables**: `withdraw*` crea la promesa,
//! el `ft_transfer` del token se ejecuta (resultado al azar) y despues corre `on_withdraw` con ese resultado; entre
//! cualquiera de ellos pueden pasar otras operaciones (depositos, pagos del agente, disputas, compactaciones...).
//!
//! Despues de cada paso:
//! - **lo que entra es lo que sale mas lo retenido**: `entrado == salido_confirmado + tvl_global`, donde `entrado` es
//!   lo que el token dejo en el contrato por `ft_transfer_call` (authorize y depositos) y `salido_confirmado` lo que
//!   salio por retiros con callback Ok;
//! - `tvl_global == sum(en escrow, incluida la disputa) + sum(claimable) + in_flight + sum(presupuestos)`, con
//!   `budgets_total` igual a la suma de los presupuestos, `tvl_operador` y `payer_tvl` iguales a lo en escrow mas los
//!   presupuestos de cada uno, e `in_flight` igual a la suma de los retiros pendientes;
//! - contra el saldo del token (modelado): `tvl_global <= ft_balance_of(contrato) + sum(transferidos sin callback)`;
//! - los topes: `tvl_operador <= tope_tvl_operador`, `tvl_global <= tope_tvl_global`, y ningun presupuesto gasta mas
//!   de su tope diario en un dia;
//! - toda llamada rechazada (panic o `Value(amount)`) deja el estado igual.

use super::common::*;
use crate::*;
use proptest::prelude::*;

const PAYERS: [&str; 3] = ["p0.testnet", "p1.testnet", "p2.testnet"];
const RECEIVERS: [&str; 2] = ["r0.testnet", "r1.testnet"];
const AGENTS: [&str; 2] = [AGENT, "agent-dos.testnet"];
const FEES2: &str = "kk-fees.testnet";
const ARBITER2: &str = "kk-arbiter.testnet";
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
        dispute_temprana: bool,
    },
    Deposit {
        owner: usize,
        agent: usize,
        op: usize,
        amount: u128,
        tope_por_pago: u128,
        tope_diario: u128,
    },
    Afb {
        owner: usize,
        agent: usize,
        op: usize,
        receiver: usize,
        amount: u128,
        salt: u16,
        /// `Some(i)`: usa el presupuesto `i` de los depositados (si hay), en lugar de `owner`/`agent`/`op`.
        existente: Option<usize>,
    },
    WithdrawBudget {
        owner: usize,
        agent: usize,
        op: usize,
    },
    Revoke {
        owner: usize,
        agent: usize,
        op: usize,
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
    OpenDispute {
        pick: usize,
        por_payer: bool,
    },
    Resolve {
        pick: usize,
        to_receiver: u128,
    },
    ResolveExpired {
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
        horas: u64,
    },
    Compact {
        pick: usize,
    },
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        4 => (0..3usize, 0..2usize, 0..2usize, 1..3_000_000u128, any::<u16>(), any::<bool>())
            .prop_map(|(payer, receiver, op, amount, salt, dispute_temprana)| Op::Authorize { payer, receiver, op, amount, salt, dispute_temprana }),
        3 => (0..3usize, 0..2usize, 0..2usize, 1..6_000_000u128, 1..3_000_000u128, 1..5_000_000u128)
            .prop_map(|(owner, agent, op, amount, tope_por_pago, tope_diario)| Op::Deposit { owner, agent, op, amount, tope_por_pago, tope_diario }),
        5 => (0..3usize, 0..2usize, 0..2usize, 0..2usize, 1..3_000_000u128, any::<u16>(), prop::option::weighted(0.8, any::<usize>()))
            .prop_map(|(owner, agent, op, receiver, amount, salt, existente)| Op::Afb { owner, agent, op, receiver, amount, salt, existente }),
        1 => (0..3usize, 0..2usize, 0..2usize).prop_map(|(owner, agent, op)| Op::WithdrawBudget { owner, agent, op }),
        1 => (0..3usize, 0..2usize, 0..2usize).prop_map(|(owner, agent, op)| Op::Revoke { owner, agent, op }),
        3 => (any::<usize>(), prop::option::of(1..2_000_000u128)).prop_map(|(pick, amount)| Op::Release { pick, amount }),
        1 => (any::<usize>(), any::<bool>()).prop_map(|(pick, por_receiver)| Op::Refund { pick, por_receiver }),
        1 => any::<usize>().prop_map(|pick| Op::Reclaim { pick }),
        2 => (any::<usize>(), any::<bool>()).prop_map(|(pick, por_payer)| Op::OpenDispute { pick, por_payer }),
        2 => (any::<usize>(), 0..3_000_000u128).prop_map(|(pick, to_receiver)| Op::Resolve { pick, to_receiver }),
        1 => any::<usize>().prop_map(|pick| Op::ResolveExpired { pick }),
        3 => (0..CUENTAS.len()).prop_map(|cuenta| Op::Withdraw { cuenta }),
        3 => (any::<usize>(), any::<bool>()).prop_map(|(pick, ok)| Op::EjecutarTransfer { pick, ok }),
        3 => any::<usize>().prop_map(|pick| Op::Callback { pick }),
        1 => (1..500_000u128).prop_map(|amount| Op::Donate { amount }),
        2 => (1..60u64).prop_map(|horas| Op::Avanzar { horas }),
        // Saltos largos: sin ellos nunca se llega a la retencion de `compact` (2 dias + 7).
        1 => (72..240u64).prop_map(|horas| Op::Avanzar { horas }),
        1 => any::<usize>().prop_map(|pick| Op::Compact { pick }),
    ]
}

fn nombre(o: &Op) -> &'static str {
    match o {
        Op::Authorize { .. } => "authorize",
        Op::Deposit { .. } => "deposit",
        Op::Afb { .. } => "authorize_from_budget",
        Op::WithdrawBudget { .. } => "withdraw_budget",
        Op::Revoke { .. } => "revoke_agent",
        Op::Release { .. } => "release",
        Op::Refund { .. } => "refund",
        Op::Reclaim { .. } => "reclaim",
        Op::OpenDispute { .. } => "open_dispute",
        Op::Resolve { .. } => "resolve_dispute",
        Op::ResolveExpired { .. } => "resolve_expired_dispute",
        Op::Withdraw { .. } => "withdraw",
        Op::EjecutarTransfer { .. } => "ejecutar_transfer",
        Op::Callback { .. } => "on_withdraw",
        Op::Donate { .. } => "donate",
        Op::Avanzar { .. } => "avanzar",
        Op::Compact { .. } => "compact",
    }
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
    /// Lo que el token dejo en el contrato por `ft_transfer_call` (authorize y depositos).
    entrado: u128,
    /// Lo que salio por retiros con callback Ok.
    salido_confirmado: u128,
    pendientes: Vec<Pendiente>,
    ids: Vec<PaymentId>,
    /// Lo autorizado por presupuesto en el modelo, por `(owner, agent, op, generacion, dia)`; la generacion sube con
    /// cada `revoke_agent` (un presupuesto nuevo empieza de cero).
    gasto_dia: std::collections::HashMap<(usize, usize, usize, u32, u64), u128>,
    generacion: std::collections::HashMap<(usize, usize, usize), u32>,
    /// Los presupuestos depositados alguna vez, `(owner, agent, op)`.
    depositados: Vec<(usize, usize, usize)>,
    /// Cuantas veces salio bien cada operacion (para afirmar que el proptest ejercita todo).
    exitos: std::collections::BTreeMap<&'static str, u32>,
}

/// Lo que una llamada rechazada no puede cambiar.
type Huella = (
    u128,
    u128,
    u128,
    u32,
    u128,
    Vec<u128>,
    Vec<(u128, u128)>,
    Vec<Option<Budget>>,
    Vec<Payment>,
);

fn huella(c: &Arras) -> Huella {
    let claim: u128 = CUENTAS.iter().map(|a| c.claimable_of(&acc(a))).sum();
    let mut por_payer = vec![];
    let mut budgets = vec![];
    for op in OPS {
        for p in PAYERS {
            por_payer.push(c.payer_tvl_of(op, &acc(p)));
            for a in AGENTS {
                budgets.push(c.budget_of(p, a, op));
            }
        }
    }
    let ops = OPS
        .iter()
        .map(|o| {
            let op = c.operators.get(*o).unwrap();
            (op.tvl_operador.0, op.sponsor_pool.0)
        })
        .collect();
    (
        c.tvl_global,
        c.in_flight,
        c.budgets_total,
        c.payments.len(),
        claim,
        por_payer,
        ops,
        budgets,
        c.payments.values().cloned().collect(),
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
        let mut c = new_contract();
        for (op, fees, arbiter) in [("em", FEES, ARBITER), ("kk", FEES2, ARBITER2)] {
            ctx(OWNER, 1, NOW);
            let mut i = operator_input();
            i.fee_recipient = acc(fees);
            i.arbiter = acc(arbiter);
            if op == "kk" {
                i.fee_bps = 1800;
                i.tope_tvl_operador = U128(8 * USDC_UNIT);
                i.arbiter_window_ns = U64(MIN_ARBITER_WINDOW_NS);
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
            entrado: 0,
            salido_confirmado: 0,
            pendientes: vec![],
            ids: vec![],
            gasto_dia: Default::default(),
            generacion: Default::default(),
            depositados: vec![],
            exitos: Default::default(),
        }
    }

    fn pago(&self, pick: usize) -> Option<(PaymentId, Payment)> {
        if self.ids.is_empty() {
            return None;
        }
        let id = self.ids[pick % self.ids.len()].clone();
        self.c.payments.get(&id).cloned().map(|p| (id, p))
    }

    /// `ft_on_transfer` desde el token: la huella no cambia si rechaza; lo usado entra al contrato.
    fn ft(&mut self, sender: &str, amount: u128, msg: &str) -> bool {
        ctx(USDC, 0, self.now);
        let antes = huella(&self.c);
        let back = call_ft_on_transfer(&mut self.c, sender, amount, msg);
        if back == amount {
            assert_eq!(
                huella(&self.c),
                antes,
                "un rechazo de ft_on_transfer no cambia el estado"
            );
        } else {
            assert_eq!(back, 0, "o todo o nada");
        }
        self.saldo_token += amount - back;
        self.entrado += amount - back;
        back == 0
    }

    fn aplicar(&mut self, o: &Op) {
        let antes = huella(&self.c);
        self.aplicar_sin_contar(o);
        if huella(&self.c) != antes {
            *self.exitos.entry(nombre(o)).or_default() += 1;
        }
    }

    fn aplicar_sin_contar(&mut self, o: &Op) {
        let now = self.now;
        match o.clone() {
            Op::Authorize {
                payer,
                receiver,
                op,
                amount,
                salt,
                dispute_temprana,
            } => {
                let mut m = msg_v1();
                m.operator_id = OPS[op].into();
                m.receiver = acc(RECEIVERS[receiver]);
                m.release_deadline = now + DAY_NS;
                m.reclaim_after = now + 2 * DAY_NS;
                m.fee_bps_max = 1800;
                m.salt[..2].copy_from_slice(&salt.to_le_bytes());
                let dd = dispute_temprana.then_some(now + DAY_NS / 2);
                let msg = msg_v2(m, dd, None);
                if self.ft(PAYERS[payer], amount, &msg) {
                    self.ids.push(id_of(PAYERS[payer], amount, &msg));
                }
            }
            Op::Deposit {
                owner,
                agent,
                op,
                amount,
                tope_por_pago,
                tope_diario,
            } => {
                let mut b = budget_msg(AGENTS[agent], tope_por_pago, tope_diario);
                b.operator_id = OPS[op].into();
                if self.ft(PAYERS[owner], amount, &b.to_canonical_json()) {
                    self.depositados.push((owner, agent, op));
                }
            }
            Op::Afb {
                owner,
                agent,
                op,
                receiver,
                amount,
                salt,
                existente,
            } => {
                let (owner, agent, op) = match existente {
                    Some(i) if !self.depositados.is_empty() => {
                        self.depositados[i % self.depositados.len()]
                    }
                    _ => (owner, agent, op),
                };
                let mut a = afb(amount, 0);
                a.operator_id = OPS[op].into();
                a.receiver = RECEIVERS[receiver].into();
                a.release_deadline = now + DAY_NS;
                a.reclaim_after = now + 2 * DAY_NS;
                a.fee_bps_max = 1800;
                let mut s = [0u8; 16];
                s[..2].copy_from_slice(&salt.to_le_bytes());
                s[15] = 0xbb;
                a.salt = crate::msg::hex_encode(&s);
                ctx(AGENTS[agent], 0, now);
                let owner_s = PAYERS[owner];
                if intentar(self, |c| {
                    call_afb(c, owner_s, &a);
                }) {
                    self.ids.push(afb_id(owner_s, &a));
                    let g = self
                        .generacion
                        .get(&(owner, agent, op))
                        .copied()
                        .unwrap_or(0);
                    let gastado = self
                        .gasto_dia
                        .entry((owner, agent, op, g, now / DAY_NS))
                        .or_default();
                    *gastado += amount;
                    // El contrato lleva la cuenta del dia igual que el modelo, y no pasa el tope.
                    let b = self.c.budget_of(owner_s, AGENTS[agent], OPS[op]).unwrap();
                    assert_eq!(b.gastado_hoy.0, *gastado, "gastado_hoy");
                    assert!(b.gastado_hoy.0 <= b.tope_diario.0, "tope diario");
                    assert!(amount <= b.tope_por_pago.0, "tope por pago del presupuesto");
                }
            }
            Op::WithdrawBudget { owner, agent, op } => {
                ctx(PAYERS[owner], 0, now);
                intentar(self, |c| {
                    c.withdraw_budget(acc(AGENTS[agent]), OPS[op].into())
                });
            }
            Op::Revoke { owner, agent, op } => {
                ctx(PAYERS[owner], 0, now);
                if intentar(self, |c| c.revoke_agent(acc(AGENTS[agent]), OPS[op].into())) {
                    *self.generacion.entry((owner, agent, op)).or_default() += 1;
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
            Op::OpenDispute { pick, por_payer } => {
                let Some((id, p)) = self.pago(pick) else {
                    return;
                };
                let quien = if por_payer { p.payer } else { p.receiver };
                ctx(quien.as_str(), 0, now);
                intentar(self, |c| c.open_dispute(id, "ab".repeat(32)));
            }
            Op::Resolve { pick, to_receiver } => {
                let Some((id, p)) = self.pago(pick) else {
                    return;
                };
                let arbiter = self
                    .c
                    .operators
                    .get(&p.operator_id)
                    .unwrap()
                    .arbiter
                    .clone();
                // La mitad de las veces un reparto exacto; la otra, el que salio (casi siempre E63).
                let live = p.live_amount();
                let (r, py) = if to_receiver % 2 == 0 {
                    let r = to_receiver.min(live);
                    (r, live - r)
                } else {
                    (to_receiver, live / 2)
                };
                ctx(arbiter.as_str(), 0, now);
                intentar(self, |c| c.resolve_dispute(id, U128(r), U128(py)));
            }
            Op::ResolveExpired { pick } => {
                let Some((id, _)) = self.pago(pick) else {
                    return;
                };
                ctx(OTHER, 0, now);
                intentar(self, |c| c.resolve_expired_dispute(id));
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
                let ok = p.ejecutado == Some(true);
                let devuelto = on_withdraw_con(&mut self.c, p.cuenta.as_str(), p.amount, ok, now);
                assert_eq!(devuelto, if ok { p.amount } else { 0 });
                if ok {
                    self.salido_confirmado += p.amount;
                }
            }
            Op::Donate { amount } => self.saldo_token += amount,
            Op::Avanzar { horas } => self.now += horas * 3_600 * 1_000_000_000,
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
        assert_invariante(c, &CUENTAS);
        assert_eq!(
            self.entrado,
            self.salido_confirmado + c.tvl_global,
            "lo que entra es lo que sale mas lo retenido"
        );
        assert_eq!(
            c.in_flight,
            self.pendientes.iter().map(|p| p.amount).sum::<u128>(),
            "in_flight"
        );
        for op in OPS {
            let o = c.operators.get(op).unwrap();
            assert!(
                o.tvl_operador.0 <= o.tope_tvl_operador.0,
                "tope del operador {op}"
            );
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

    /// U-16 `invariante_proptest` de v2.
    #[test]
    fn u16_invariante_proptest(ops in prop::collection::vec(op(), 1..50)) {
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
            on_withdraw_con(&mut m.c, p.cuenta.as_str(), p.amount, ok, m.now);
            if ok {
                m.salido_confirmado += p.amount;
            }
            m.verificar();
        }
    }
}

/// El proptest no pasa "por vacio": con una semilla fija, en 400 secuencias cada operacion que cambia el estado del
/// contrato sale bien al menos una vez (incluidos el pago del agente, las dos formas de resolver una disputa, la
/// revocacion, el retiro fallido y `compact`).
#[test]
fn el_proptest_ejercita_cada_operacion() {
    use proptest::strategy::ValueTree;
    use proptest::test_runner::TestRunner;
    let mut runner = TestRunner::deterministic();
    let mut total: std::collections::BTreeMap<&'static str, u32> = Default::default();
    let mut fallidos = 0u32;
    for _ in 0..400 {
        let ops = prop::collection::vec(op(), 1..50)
            .new_tree(&mut runner)
            .unwrap()
            .current();
        let mut m = Modelo::nuevo();
        for o in &ops {
            if matches!(o, Op::Callback { .. }) {
                let listos: Vec<bool> = m.pendientes.iter().filter_map(|p| p.ejecutado).collect();
                fallidos += listos.iter().filter(|ok| !**ok).count() as u32;
            }
            m.aplicar(o);
        }
        for (k, v) in m.exitos {
            *total.entry(k).or_default() += v;
        }
    }
    for k in [
        "authorize",
        "deposit",
        "authorize_from_budget",
        "withdraw_budget",
        "revoke_agent",
        "release",
        "refund",
        "reclaim",
        "open_dispute",
        "resolve_dispute",
        "resolve_expired_dispute",
        "withdraw",
        "on_withdraw",
        "compact",
    ] {
        assert!(
            total.get(k).copied().unwrap_or(0) > 0,
            "{k} nunca salio bien: {total:?}"
        );
    }
    assert!(fallidos > 0, "algun retiro fallido llega al callback");
    println!("MEDIDA exitos por operacion en 400 secuencias: {total:?}");
}
