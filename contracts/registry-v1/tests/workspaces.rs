//! Tests del registro contra las wasm reales en el sandbox de near-workspaces.
//!
//! - Pagos de **Arras v1 de verdad** (`contracts/escrow-v1`, con el FT de prueba de `contracts/test-ft`): authorize por
//!   `ft_transfer_call`, `release`, `refund` y `reclaim` reales.
//! - El escrow de mentira (`contracts/test-escrow`) para lo que v1 no puede dar: payer == receiver, un JSON roto, una
//!   respuesta de mas de 8 KiB, un `status` de v2.
//! - Una cuenta **sin codigo** en la lista de escrows: su `get_payment` falla de verdad (`E20`).
//! - Sin red: `NEAR_SANDBOX_BIN_PATH` apuntando a un `near-sandbox` 2.13.4.
//!
//! Las mediciones de gas salen con prefijo `MEDIDA`: `cargo test -p arras-registry-v1 --test workspaces -- --nocapture`.

use near_workspaces::network::Sandbox;
use near_workspaces::operations::Function;
use near_workspaces::result::ExecutionFinalResult;
use near_workspaces::types::{Gas, NearToken};
use near_workspaces::{Account, AccountId, Worker};
use serde_json::{Value, json};
use tokio::sync::{OnceCell, Semaphore};

static WASM_REG: OnceCell<Vec<u8>> = OnceCell::const_new();
static WASM_V1: OnceCell<Vec<u8>> = OnceCell::const_new();
static WASM_FT: OnceCell<Vec<u8>> = OnceCell::const_new();
static WASM_MOCK: OnceCell<Vec<u8>> = OnceCell::const_new();
/// Cuantos sandboxes a la vez (cada test levanta el suyo).
static SANDBOXES: Semaphore = Semaphore::const_new(4);

const USDC: u128 = 1_000_000;
const ONE_YOCTO: NearToken = NearToken::from_yoctonear(1);
const DAY_NS: u64 = 86_400 * 1_000_000_000;
const BYTE: u128 = 10_000_000_000_000_000_000;
const HASH: &str = "00ff10a0b1c2d3e4f5061728394a5b6c7d8e9fa0b1c2d3e4f5061728394a5b6c";

async fn wasm(
    cell: &'static OnceCell<Vec<u8>>,
    path: &'static str,
) -> anyhow::Result<&'static [u8]> {
    let w = cell
        .get_or_try_init(|| async { near_workspaces::compile_project(path).await })
        .await?;
    Ok(w.as_slice())
}

async fn wasm_reg() -> anyhow::Result<&'static [u8]> {
    wasm(&WASM_REG, "./").await
}

struct Env {
    w: Worker<Sandbox>,
    root: Account,
    ft: Account,
    arras: Account,
    registry: Account,
    mock: Account,
    nocode: Account,
    capturer: Account,
    payer: Account,
    worker: Account,
    tercero: Account,
    now: u64,
    deposit: u128,
}

async fn sub(root: &Account, name: &str, near: u128) -> anyhow::Result<Account> {
    Ok(root
        .create_subaccount(name)
        .initial_balance(NearToken::from_near(near))
        .transact()
        .await?
        .into_result()?)
}

/// Deploy + `new` en una transaccion (lo que hace `with-init-call` en el plan de deploy).
async fn deploy_registry(
    root: &Account,
    name: &str,
    escrows: &[&AccountId],
) -> anyhow::Result<Account> {
    let acc = sub(root, name, 10).await?;
    let r = acc
        .batch(acc.id())
        .deploy(wasm_reg().await?)
        .call(
            Function::new("new")
                .args_json(json!({ "escrows": escrows }))
                .gas(Gas::from_tgas(30)),
        )
        .transact()
        .await?;
    assert!(r.is_success(), "deploy + new: {:?}", r.into_result());
    Ok(acc)
}

async fn call_ok(
    from: &Account,
    to: &AccountId,
    method: &str,
    args: Value,
    dep: NearToken,
) -> anyhow::Result<()> {
    let r = from
        .call(to, method)
        .args_json(args)
        .deposit(dep)
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;
    assert!(r.is_success(), "{method}: {:?}", r.into_result());
    Ok(())
}

async fn setup() -> anyhow::Result<Env> {
    let w = near_workspaces::sandbox().await?;
    let root = w.root_account()?;
    let ft = sub(&root, "ft", 20).await?;
    assert!(
        ft.deploy(wasm(&WASM_FT, "../test-ft").await?)
            .await?
            .is_success()
    );
    call_ok(
        &ft,
        ft.id(),
        "new",
        json!({ "owner_id": ft.id(), "total_supply": (1_000_000 * USDC).to_string() }),
        NearToken::from_yoctonear(0),
    )
    .await?;
    let owner = sub(&root, "owner", 10).await?;
    let capturer = sub(&root, "capturer", 10).await?;
    let fees = sub(&root, "fees", 5).await?;
    let payer = sub(&root, "payer", 20).await?;
    let worker = sub(&root, "worker", 20).await?;
    let tercero = sub(&root, "tercero", 20).await?;
    let arras = sub(&root, "arras", 10).await?;
    let r = arras
        .batch(arras.id())
        .deploy(wasm(&WASM_V1, "../escrow-v1").await?)
        .call(
            Function::new("new")
                .args_json(json!({
                    "owner": owner.id(),
                    "guardian": owner.id(),
                    "token": ft.id(),
                    "tope_por_pago": (10 * USDC).to_string(),
                    "tope_tvl_global": (1_000 * USDC).to_string(),
                }))
                .gas(Gas::from_tgas(30)),
        )
        .transact()
        .await?;
    assert!(r.is_success(), "deploy v1: {:?}", r.into_result());
    for who in [arras.id(), payer.id(), worker.id(), fees.id()] {
        call_ok(
            &root,
            ft.id(),
            "storage_deposit",
            json!({ "account_id": who, "registration_only": true }),
            NearToken::from_millinear(13),
        )
        .await?;
    }
    call_ok(
        &ft,
        ft.id(),
        "ft_transfer",
        json!({ "receiver_id": payer.id(), "amount": (100 * USDC).to_string() }),
        ONE_YOCTO,
    )
    .await?;
    call_ok(
        &owner,
        arras.id(),
        "register_operator",
        json!({ "operator_id": "em", "input": {
            "owner": owner.id(),
            "capturers": [capturer.id()],
            "fee_bps": 1300,
            "fee_recipient": fees.id(),
            "min_amount": "10000",
            "tope_tvl_operador": (200 * USDC).to_string(),
            "tope_tvl_por_payer": (50 * USDC).to_string(),
            "sponsor_max_por_payer": "0",
        }}),
        ONE_YOCTO,
    )
    .await?;
    call_ok(
        &payer,
        arras.id(),
        "storage_deposit",
        json!({}),
        NearToken::from_millinear(500),
    )
    .await?;
    let mock = sub(&root, "mock", 10).await?;
    assert!(
        mock.deploy(wasm(&WASM_MOCK, "../test-escrow").await?)
            .await?
            .is_success()
    );
    call_ok(
        &mock,
        mock.id(),
        "new",
        json!({}),
        NearToken::from_yoctonear(0),
    )
    .await?;
    let nocode = sub(&root, "nocode", 1).await?;
    let registry =
        deploy_registry(&root, "registry", &[arras.id(), mock.id(), nocode.id()]).await?;
    let cfg: Value = root.view(registry.id(), "get_config").await?.json()?;
    let deposit: u128 = cfg["feedback_deposit"].as_str().unwrap().parse()?;
    let now = w.view_block().await?.timestamp();
    Ok(Env {
        w,
        root,
        ft,
        arras,
        registry,
        mock,
        nocode,
        capturer,
        payer,
        worker,
        tercero,
        now,
        deposit,
    })
}

fn msg_for(receiver: &AccountId, release: u64, reclaim: u64, salt: u8) -> String {
    let mut s = [0u8; 16];
    s[15] = salt;
    arras_escrow_core::AuthorizeMsg {
        operator_id: "em".into(),
        receiver: receiver.as_str().parse().unwrap(),
        release_deadline: release,
        reclaim_after: reclaim,
        fee_bps_max: 1300,
        terms_hash: [0; 32],
        salt: s,
    }
    .to_canonical_json()
}

/// Lo que un test le pide a `give_feedback`.
#[derive(Clone)]
struct Pedido<'a> {
    escrow: &'a AccountId,
    payment_id: String,
    score: u8,
    gas: Gas,
    deposit: u128,
}

impl Env {
    fn msg(&self, salt: u8) -> String {
        msg_for(
            self.worker.id(),
            self.now + DAY_NS,
            self.now + 2 * DAY_NS,
            salt,
        )
    }

    /// authorize real (payer -> worker) que tiene que pasar; devuelve el `payment_id`.
    async fn authorize(&self, amount: u128, msg: &str) -> anyhow::Result<String> {
        let r = self
            .payer
            .call(self.ft.id(), "ft_transfer_call")
            .args_json(
                json!({ "receiver_id": self.arras.id(), "amount": amount.to_string(), "msg": msg }),
            )
            .deposit(ONE_YOCTO)
            .gas(Gas::from_tgas(100))
            .transact()
            .await?;
        let used: String = r.clone().json()?;
        assert_eq!(
            used,
            amount.to_string(),
            "authorize rechazado: {:?}",
            r.logs()
        );
        Ok(self
            .root
            .view(self.arras.id(), "compute_payment_id")
            .args_json(
                json!({ "sender_id": self.payer.id(), "amount": amount.to_string(), "msg": msg }),
            )
            .await?
            .json()?)
    }

    async fn capture(&self, method: &str, id: &str, amount: Option<u128>) -> anyhow::Result<()> {
        let args = if method == "release" {
            json!({ "payment_id": id, "amount": amount.map(|a| a.to_string()) })
        } else {
            json!({ "payment_id": id })
        };
        call_ok(
            &self.capturer,
            self.arras.id(),
            method,
            args,
            NearToken::from_yoctonear(0),
        )
        .await
    }

    /// Un pago de 1 USDC liberado entero.
    async fn released(&self, salt: u8) -> anyhow::Result<String> {
        let id = self.authorize(USDC, &self.msg(salt)).await?;
        self.capture("release", &id, None).await?;
        Ok(id)
    }

    fn pedido(&self, payment_id: &str) -> Pedido<'_> {
        Pedido {
            escrow: self.arras.id(),
            payment_id: payment_id.into(),
            score: 90,
            gas: Gas::from_tgas(100),
            deposit: self.deposit,
        }
    }

    async fn give(&self, rater: &Account, p: &Pedido<'_>) -> anyhow::Result<ExecutionFinalResult> {
        Ok(rater
            .call(self.registry.id(), "give_feedback")
            .args_json(json!({
                "escrow": p.escrow,
                "payment_id": p.payment_id,
                "score": p.score,
                "tag1": "quality",
                "tag2": "successRate",
                "feedback_uri": "https://em.example/feedback/1.json",
                "feedback_hash": HASH,
            }))
            .deposit(NearToken::from_yoctonear(p.deposit))
            .gas(p.gas)
            .transact()
            .await?)
    }

    async fn payment(&self, id: &str) -> anyhow::Result<Value> {
        Ok(self
            .root
            .view(self.arras.id(), "get_payment")
            .args_json(json!({ "payment_id": id }))
            .await?
            .json()?)
    }

    async fn view(&self, method: &str, args: Value) -> anyhow::Result<Value> {
        Ok(self
            .root
            .view(self.registry.id(), method)
            .args_json(args)
            .await?
            .json()?)
    }

    /// Los reembolsos de gas (y el deposito de una tx que fallo) llegan en recibos que se ejecutan DESPUES de que
    /// `transact()` vuelve (medido: el reembolso del deploy + `new` del registro, +0,0434 NEAR, aparece un bloque
    /// despues). Antes de leer saldos se dejan pasar unos bloques.
    async fn settle(&self) -> anyhow::Result<()> {
        self.w.fast_forward(3).await?;
        Ok(())
    }

    async fn balance(&self, a: &AccountId) -> anyhow::Result<u128> {
        Ok(self.w.view_account(a).await?.balance.as_yoctonear())
    }

    /// Saldo del registro menos lo que cuesta su storage.
    async fn free(&self) -> anyhow::Result<i128> {
        let a = self.w.view_account(self.registry.id()).await?;
        Ok(a.balance.as_yoctonear() as i128 - (u128::from(a.storage_usage) * BYTE) as i128)
    }

    async fn storage(&self) -> anyhow::Result<u64> {
        Ok(self.w.view_account(self.registry.id()).await?.storage_usage)
    }

    /// `give_feedback` midiendo: (resultado, gasto del rater, bytes que crecio el registro, cambio del saldo libre del
    /// registro, tope de la recompensa de gas del registro).
    async fn give_medido(&self, rater: &Account, p: &Pedido<'_>) -> anyhow::Result<Medida> {
        self.settle().await?;
        let b0 = self.balance(rater.id()).await?;
        let s0 = self.storage().await?;
        let f0 = self.free().await?;
        let r = self.give(rater, p).await?;
        self.settle().await?;
        let gasto = b0 - self.balance(rater.id()).await?;
        let bytes = self.storage().await? as i64 - s0 as i64;
        let libre = self.free().await? - f0;
        let recompensa: u128 = r
            .outcomes()
            .iter()
            .filter(|o| &o.executor_id == self.registry.id())
            .map(|o| o.tokens_burnt.as_yoctonear())
            .sum();
        Ok(Medida {
            r,
            gasto,
            bytes,
            libre,
            recompensa,
        })
    }
}

struct Medida {
    r: ExecutionFinalResult,
    gasto: u128,
    bytes: i64,
    libre: i128,
    recompensa: u128,
}

impl Medida {
    /// El registro no subsidia ni se queda con el deposito: su saldo libre sube a lo sumo la recompensa de gas.
    fn registro_neutro(&self) {
        assert!(
            self.libre >= 0 && self.libre <= self.recompensa as i128,
            "saldo libre del registro {} fuera de [0, {}]",
            self.libre,
            self.recompensa
        );
    }
}

fn reg_events(r: &ExecutionFinalResult, event: &str) -> Vec<Value> {
    r.logs()
        .iter()
        .filter_map(|l| l.strip_prefix("EVENT_JSON:"))
        .filter_map(|j| serde_json::from_str::<Value>(j).ok())
        .filter(|e| e["standard"] == "arras-registry" && e["event"] == event)
        .collect()
}

fn rechazos(r: &ExecutionFinalResult) -> Vec<String> {
    r.logs()
        .iter()
        .filter_map(|l| l.strip_prefix("arras-registry:rechazo:"))
        .map(String::from)
        .collect()
}

fn tg(g: Gas) -> f64 {
    g.as_gas() as f64 / 1e12
}

/// El callback rechazo con `codigo`: la tx sale bien, devuelve `null`, el deposito vuelve y no hay evento.
fn assert_rechazo(e: &Env, m: &Medida, codigo: &str) {
    assert!(m.r.is_success(), "{:?}", m.r.clone().into_result());
    let out: Option<u32> = m.r.clone().json().unwrap();
    assert_eq!(out, None);
    assert_eq!(rechazos(&m.r), vec![codigo.to_string()], "{:?}", m.r.logs());
    assert!(reg_events(&m.r, "feedback_given").is_empty());
    assert_eq!(m.bytes, 0, "sin storage nuevo (el candado se borra)");
    m.registro_neutro();
    assert!(
        m.gasto < e.deposit / 2,
        "el deposito vuelve: gasto {}",
        m.gasto
    );
    let fallas: Vec<_> =
        m.r.receipt_failures()
            .into_iter()
            .filter(|o| &o.executor_id == e.registry.id())
            .collect();
    assert!(
        fallas.is_empty(),
        "ningun recibo del registro falla: {fallas:?}"
    );
}

/// Una guarda de `give_feedback` antes de la llamada cruzada: la tx falla con `ARRAS_REG_<codigo>` y nada cambia.
fn assert_falla(r: &ExecutionFinalResult, codigo: &str) {
    assert!(r.is_failure(), "tenia que fallar con {codigo}");
    let err = format!("{:?}", r.clone().into_result().unwrap_err());
    assert!(err.contains(&format!("ARRAS_REG_{codigo}")), "{err}");
}

// ------------------------------------------------------------------------------------------------ deploy

/// WR-00: `new` es privado (un ajeno no inicializa), una sola vez; sin llaves la cuenta no se redespliega y el
/// registro sigue contestando. Es el ensayo del plan de deploy del handoff.
#[tokio::test]
async fn wr00_new_privado_y_sin_llaves() -> anyhow::Result<()> {
    let _permiso = SANDBOXES.acquire().await?;
    let w = near_workspaces::sandbox().await?;
    let root = w.root_account()?;
    let contract = w.dev_deploy(wasm_reg().await?).await?;
    let ajeno = w.dev_create_account().await?;
    let args = json!({ "escrows": ["v1.arras-escrow.test.near"] });
    let r = ajeno
        .call(contract.id(), "new")
        .args_json(&args)
        .transact()
        .await?;
    let err = format!("{:?}", r.into_result().unwrap_err());
    assert!(err.contains("Method new is private"), "{err}");
    assert!(
        contract.view("get_config").await.is_err(),
        "sin init no hay estado"
    );
    let ok = contract.call("new").args_json(&args).transact().await?;
    assert!(ok.is_success(), "{:?}", ok.into_result());
    let again = contract.call("new").args_json(&args).transact().await?;
    assert!(again.is_failure(), "una sola vez");
    let mal = contract
        .call("new")
        .args_json(json!({ "escrows": [] }))
        .transact()
        .await?;
    assert!(mal.is_failure());

    // El plan del handoff: deploy + new en una tx y despues borrar todas las llaves.
    let reg = deploy_registry(&root, "registry", &[&"v1.arras-escrow.test.near".parse()?]).await?;
    let keys = w.view_access_keys(reg.id()).await?;
    assert_eq!(keys.len(), 1);
    let r = reg
        .batch(reg.id())
        .delete_key(keys[0].public_key.clone())
        .transact()
        .await?;
    assert!(r.is_success(), "{:?}", r.into_result());
    assert!(w.view_access_keys(reg.id()).await?.is_empty());
    let r = reg
        .batch(reg.id())
        .deploy(wasm_reg().await?)
        .transact()
        .await;
    assert!(
        r.is_err() || r.unwrap().is_failure(),
        "sin llaves no hay redeploy"
    );
    let cfg: Value = root.view(reg.id(), "get_config").await?.json()?;
    assert_eq!(cfg["version"], "arras-registry-v1");
    assert_eq!(cfg["escrows"], json!(["v1.arras-escrow.test.near"]));
    Ok(())
}

// ------------------------------------------------------------------------------------------------ identidad

/// WR-01: alta y cambio de identidad con el storage cobrado y devuelto.
#[tokio::test]
async fn wr01_identidad() -> anyhow::Result<()> {
    let _permiso = SANDBOXES.acquire().await?;
    let e = setup().await?;
    let uri = "https://agente.example/card.json";
    let meta = "x".repeat(600);
    e.settle().await?;
    let s0 = e.storage().await?;
    let f0 = e.free().await?;
    let b0 = e.balance(e.payer.id()).await?;
    let r = e
        .payer
        .call(e.registry.id(), "register")
        .args_json(json!({ "agent_uri": uri, "metadata": meta }))
        .deposit(NearToken::from_millinear(100))
        .transact()
        .await?;
    assert!(r.is_success(), "{:?}", r.clone().into_result());
    e.settle().await?;
    let v: Value = r.clone().json()?;
    assert_eq!(v["account_id"], e.payer.id().as_str());
    assert_eq!(v["agent_uri"], uri);
    let ev = reg_events(&r, "agent_registered");
    assert_eq!(ev.len(), 1);
    assert_eq!(ev[0]["data"][0]["metadata"], meta);
    let bytes = e.storage().await? - s0;
    let costo = u128::from(bytes) * BYTE;
    let gasto = b0 - e.balance(e.payer.id()).await?;
    println!(
        "MEDIDA WR-01 register: {bytes} bytes ({} NEAR), {:.3} TGas",
        costo as f64 / 1e24,
        tg(r.total_gas_burnt)
    );
    assert!(
        gasto >= costo && gasto - costo < NearToken::from_millinear(5).as_yoctonear(),
        "vuelve el sobrante"
    );
    let libre = e.free().await? - f0;
    assert!(
        libre >= 0 && libre < NearToken::from_millinear(1).as_yoctonear() as i128,
        "{libre}"
    );

    // Una segunda alta es E03; un deposito corto, E05.
    let r = e
        .payer
        .call(e.registry.id(), "register")
        .args_json(json!({ "agent_uri": uri }))
        .deposit(NearToken::from_millinear(100))
        .transact()
        .await?;
    assert_falla(&r, "E03");
    let r = e
        .worker
        .call(e.registry.id(), "register")
        .args_json(json!({ "agent_uri": uri }))
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await?;
    assert_falla(&r, "E05");

    // update que achica: vuelve lo liberado.
    e.settle().await?;
    let s1 = e.storage().await?;
    let b1 = e.balance(e.payer.id()).await?;
    let r = e
        .payer
        .call(e.registry.id(), "update")
        .args_json(json!({ "agent_uri": "ipfs://bafynueva", "metadata": null }))
        .deposit(ONE_YOCTO)
        .transact()
        .await?;
    assert!(r.is_success(), "{:?}", r.clone().into_result());
    e.settle().await?;
    assert_eq!(reg_events(&r, "agent_updated").len(), 1);
    let liberado = u128::from(s1 - e.storage().await?) * BYTE;
    assert!(liberado > 0);
    let neto = e.balance(e.payer.id()).await? as i128 - b1 as i128;
    let quemado: u128 = r
        .outcomes()
        .iter()
        .map(|o| o.tokens_burnt.as_yoctonear())
        .sum();
    assert!(
        neto > (liberado as i128)
            - 2 * (quemado as i128)
            - NearToken::from_millinear(2).as_yoctonear() as i128,
        "vuelve lo liberado: neto {neto}, liberado {liberado}"
    );
    let a: Value = e
        .view("get_agent", json!({ "account_id": e.payer.id() }))
        .await?;
    assert_eq!(a["agent_uri"], "ipfs://bafynueva");
    assert_eq!(a["metadata"], Value::Null);
    let r = e
        .payer
        .call(e.registry.id(), "update")
        .args_json(json!({ "agent_uri": "ipfs://x" }))
        .transact()
        .await?;
    assert_falla(&r, "E06");
    let lista: Value = e.view("list_agents", json!({})).await?;
    assert_eq!(lista.as_array().unwrap().len(), 1);
    Ok(())
}

// ------------------------------------------------------------------------------------------------ calificaciones

/// WR-02: una calificacion valida de cada lado sobre un pago `Released` de Arras v1, y una sobre `PartiallyReleased`.
#[tokio::test]
async fn wr02_calificaciones_de_los_dos_lados() -> anyhow::Result<()> {
    let _permiso = SANDBOXES.acquire().await?;
    let e = setup().await?;
    let id = e.released(1).await?;
    let m = e.give_medido(&e.payer, &e.pedido(&id)).await?;
    assert!(m.r.is_success(), "{:?}", m.r.clone().into_result());
    let out: Option<u32> = m.r.clone().json()?;
    assert_eq!(out, Some(0), "{:?}", m.r.logs());
    assert!(rechazos(&m.r).is_empty());
    let ev = reg_events(&m.r, "feedback_given");
    assert_eq!(ev.len(), 1);
    let d = &ev[0]["data"][0];
    assert_eq!(d["rater"], e.payer.id().as_str());
    assert_eq!(d["ratee"], e.worker.id().as_str());
    assert_eq!(d["ratee_role"], "receiver");
    assert_eq!(d["escrow"], e.arras.id().as_str());
    assert_eq!(d["payment_id"], id);
    assert_eq!(d["operator_id"], "em");
    assert_eq!(d["released"], USDC.to_string());
    assert_eq!(d["feedback_hash"], HASH);
    // Storage cobrado: lo que crecio el registro; el resto del deposito vuelve.
    assert!(m.bytes > 0 && (m.bytes as u64) <= 1643);
    let costo = m.bytes as u128 * BYTE;
    m.registro_neutro();
    assert!(m.gasto >= costo, "paga su storage");
    assert!(
        m.gasto - costo < (e.deposit - costo) / 2,
        "vuelve el sobrante: gasto {}",
        m.gasto
    );
    let gas: Vec<String> =
        m.r.receipt_outcomes()
            .iter()
            .map(|o| format!("{} {:.3}", o.executor_id, tg(o.gas_burnt)))
            .collect();
    println!(
        "MEDIDA WR-02 give_feedback (v1 real): total {:.3} TGas; recibos: {}; {} bytes ({} NEAR)",
        tg(m.r.total_gas_burnt),
        gas.join(", "),
        m.bytes,
        costo as f64 / 1e24
    );

    // El receiver califica al payer del mismo pago.
    let r = e
        .give(
            &e.worker,
            &Pedido {
                score: 40,
                ..e.pedido(&id)
            },
        )
        .await?;
    let out: Option<u32> = r.clone().json()?;
    assert_eq!(out, Some(1), "{:?}", r.logs());
    assert_eq!(
        reg_events(&r, "feedback_given")[0]["data"][0]["ratee_role"],
        "payer"
    );

    // PartiallyReleased con plata liberada.
    let parcial = e.authorize(2 * USDC, &e.msg(2)).await?;
    e.capture("release", &parcial, Some(USDC / 2)).await?;
    let p: Value = e
        .root
        .view(e.arras.id(), "get_payment")
        .args_json(json!({ "payment_id": parcial }))
        .await?
        .json()?;
    assert_eq!(p["status"], "PartiallyReleased");
    let r = e.give(&e.payer, &e.pedido(&parcial)).await?;
    let out: Option<u32> = r.clone().json()?;
    assert_eq!(out, Some(2), "{:?}", r.logs());

    // Vistas.
    let s = e
        .view("get_summary", json!({ "account_id": e.worker.id() }))
        .await?;
    assert_eq!(s["count"], 2);
    assert_eq!(s["as_receiver"]["count"], 2);
    assert_eq!(s["average_x100"], 9000);
    assert_eq!(s["last_feedback_id"], 2);
    let s = e
        .view("get_summary", json!({ "account_id": e.payer.id() }))
        .await?;
    assert_eq!(s["as_payer"]["count"], 1);
    assert_eq!(s["as_payer"]["average_x100"], 4000);
    let l = e
        .view(
            "list_feedback",
            json!({ "account_id": e.worker.id(), "from_index": 0, "limit": 10 }),
        )
        .await?;
    assert_eq!(l.as_array().unwrap().len(), 2);
    assert_eq!(l[1]["released"], (USDC / 2).to_string());
    let f = e
        .view(
            "get_feedback_for_payment",
            json!({ "escrow": e.arras.id(), "payment_id": id, "rater": e.worker.id() }),
        )
        .await?;
    assert_eq!(f["feedback_id"], 1);
    assert_eq!(f["score"], 40);
    let pend = e
        .view(
            "is_feedback_pending",
            json!({ "escrow": e.arras.id(), "payment_id": id, "rater": e.payer.id() }),
        )
        .await?;
    assert_eq!(pend, false);
    Ok(())
}

/// WR-03: rechazos con Arras v1 real: pago inexistente, Authorized, Refunded sin nada liberado, un tercero, la segunda
/// calificacion, un escrow fuera de la lista y guardas de entrada; y un Refunded tras una liberacion parcial, que vale.
#[tokio::test]
async fn wr03_rechazos_con_v1_real() -> anyhow::Result<()> {
    let _permiso = SANDBOXES.acquire().await?;
    let e = setup().await?;
    // Inexistente: un payment_id con forma valida que nadie autorizo.
    let m = e
        .give_medido(
            &e.payer,
            &e.pedido("DN5eMA3sNES7xsoy6qvy24cVF6Dmwjxpkvd2b4e8P38M"),
        )
        .await?;
    assert_rechazo(&e, &m, "E22_PAGO_INEXISTENTE");
    println!(
        "MEDIDA WR-03 rechazo E22: total {:.3} TGas, gasto del rater {} NEAR",
        tg(m.r.total_gas_burnt),
        m.gasto as f64 / 1e24
    );
    // Authorized (sin release).
    let vivo = e.authorize(USDC, &e.msg(1)).await?;
    let m = e.give_medido(&e.payer, &e.pedido(&vivo)).await?;
    assert_rechazo(&e, &m, "E23_PAGO_SIN_LIBERAR");
    let m = e.give_medido(&e.worker, &e.pedido(&vivo)).await?;
    assert_rechazo(&e, &m, "E23_PAGO_SIN_LIBERAR");
    // Refunded sin nada liberado.
    let devuelto = e.authorize(USDC, &e.msg(2)).await?;
    e.capture("refund", &devuelto, None).await?;
    let m = e.give_medido(&e.payer, &e.pedido(&devuelto)).await?;
    assert_rechazo(&e, &m, "E23_PAGO_SIN_LIBERAR");
    // Un tercero sobre un pago liberado.
    let id = e.released(3).await?;
    let m = e.give_medido(&e.tercero, &e.pedido(&id)).await?;
    assert_rechazo(&e, &m, "E24_NO_ES_PARTE");
    // Despues de un rechazo, el payer puede calificar; la segunda vez es E18 (antes de la llamada cruzada).
    let r = e.give(&e.payer, &e.pedido(&id)).await?;
    let out: Option<u32> = r.clone().json()?;
    assert_eq!(out, Some(0));
    e.settle().await?;
    let b0 = e.balance(e.payer.id()).await?;
    let r = e
        .give(
            &e.payer,
            &Pedido {
                score: 1,
                ..e.pedido(&id)
            },
        )
        .await?;
    assert_falla(&r, "E18");
    e.settle().await?;
    assert!(
        b0 - e.balance(e.payer.id()).await? < e.deposit / 2,
        "el deposito vuelve"
    );
    let f = e.view("get_feedback", json!({ "feedback_id": 0 })).await?;
    assert_eq!(f["score"], 90, "la primera no cambia");
    // Escrow fuera de la lista: el FT (o cualquier cuenta).
    let r = e
        .give(
            &e.payer,
            &Pedido {
                escrow: e.ft.id(),
                ..e.pedido(&id)
            },
        )
        .await?;
    assert_falla(&r, "E10");
    let otro_v1: AccountId = format!("v9.{}", e.root.id()).parse()?;
    let r = e
        .give(
            &e.payer,
            &Pedido {
                escrow: &otro_v1,
                ..e.pedido(&id)
            },
        )
        .await?;
    assert_falla(&r, "E10");
    // Guardas de entrada.
    let r = e
        .give(
            &e.payer,
            &Pedido {
                score: 101,
                ..e.pedido(&id)
            },
        )
        .await?;
    assert_falla(&r, "E12");
    let r = e
        .give(
            &e.payer,
            &Pedido {
                deposit: e.deposit - 1,
                ..e.pedido(&id)
            },
        )
        .await?;
    assert_falla(&r, "E16");
    let r = e
        .give(
            &e.payer,
            &Pedido {
                gas: Gas::from_gas(Gas::from_tgas(60).as_gas() - 1),
                ..e.pedido(&id)
            },
        )
        .await?;
    assert_falla(&r, "E17");
    assert_eq!(e.view("get_config", json!({})).await?["feedbacks"], 1);
    // Refunded despues de una liberacion parcial: paso plata, vale para los dos (RESPUESTA de c0der, 23:30Z).
    let parcial = e.authorize(2 * USDC, &e.msg(4)).await?;
    e.capture("release", &parcial, Some(USDC / 2)).await?;
    e.capture("refund", &parcial, None).await?;
    let p = e.payment(&parcial).await?;
    assert_eq!(p["status"], "Refunded");
    assert_eq!(p["released"], (USDC / 2).to_string());
    let m = e.give_medido(&e.payer, &e.pedido(&parcial)).await?;
    assert_eq!(
        m.r.clone().json::<Option<u32>>()?,
        Some(1),
        "{:?}",
        m.r.logs()
    );
    m.registro_neutro();
    let r = e.give(&e.worker, &e.pedido(&parcial)).await?;
    assert_eq!(r.clone().json::<Option<u32>>()?, Some(2), "{:?}", r.logs());
    let f = e.view("get_feedback", json!({ "feedback_id": 1 })).await?;
    assert_eq!(f["released"], (USDC / 2).to_string());
    Ok(())
}

/// WR-04: `Reclaimed` sin plata liberada (E23) y despues de una liberacion parcial (vale), con el reloj del sandbox
/// adelantado.
#[tokio::test]
async fn wr04_reclaimed() -> anyhow::Result<()> {
    let _permiso = SANDBOXES.acquire().await?;
    let e = setup().await?;
    let msg = msg_for(
        e.worker.id(),
        e.now + 300 * 1_000_000_000,
        e.now + 301 * 1_000_000_000,
        9,
    );
    let id = e.authorize(USDC, &msg).await?;
    let msg2 = msg_for(
        e.worker.id(),
        e.now + 300 * 1_000_000_000,
        e.now + 301 * 1_000_000_000,
        10,
    );
    let parcial = e.authorize(2 * USDC, &msg2).await?;
    e.capture("release", &parcial, Some(USDC / 4)).await?;
    let objetivo = e.now + 301 * 1_000_000_000;
    let mut bloques = 0u64;
    while e.w.view_block().await?.timestamp() <= objetivo {
        assert!(bloques < 200_000, "fast_forward no adelanta el reloj");
        e.w.fast_forward(5_000).await?;
        bloques += 5_000;
    }
    for pid in [&id, &parcial] {
        call_ok(
            &e.payer,
            e.arras.id(),
            "reclaim",
            json!({ "payment_id": pid }),
            NearToken::from_yoctonear(0),
        )
        .await?;
        assert_eq!(e.payment(pid).await?["status"], "Reclaimed");
    }
    // Sin plata liberada: E23 para los dos.
    let m = e.give_medido(&e.payer, &e.pedido(&id)).await?;
    assert_rechazo(&e, &m, "E23_PAGO_SIN_LIBERAR");
    let m = e.give_medido(&e.worker, &e.pedido(&id)).await?;
    assert_rechazo(&e, &m, "E23_PAGO_SIN_LIBERAR");
    // Reclaimed despues de una liberacion parcial: vale.
    assert_eq!(
        e.payment(&parcial).await?["released"],
        (USDC / 4).to_string()
    );
    let r = e.give(&e.payer, &e.pedido(&parcial)).await?;
    assert_eq!(r.clone().json::<Option<u32>>()?, Some(0), "{:?}", r.logs());
    let r = e.give(&e.worker, &e.pedido(&parcial)).await?;
    assert_eq!(r.clone().json::<Option<u32>>()?, Some(1), "{:?}", r.logs());
    Ok(())
}

/// WR-05: `get_payment` que falla (una cuenta sin codigo y un escrow que entra en panico) y respuestas que v1 no da:
/// calificarse a si mismo, un JSON roto, una respuesta de mas de 8 KiB y un `status` de v2.
#[tokio::test]
async fn wr05_get_payment_que_falla_y_respuestas_raras() -> anyhow::Result<()> {
    let _permiso = SANDBOXES.acquire().await?;
    let e = setup().await?;
    let m = e
        .give_medido(
            &e.payer,
            &Pedido {
                escrow: e.nocode.id(),
                ..e.pedido("cualquiera")
            },
        )
        .await?;
    assert_rechazo(&e, &m, "E20_GET_PAYMENT_FALLO");
    let m = e
        .give_medido(
            &e.payer,
            &Pedido {
                escrow: e.mock.id(),
                ..e.pedido("panic")
            },
        )
        .await?;
    assert_rechazo(&e, &m, "E20_GET_PAYMENT_FALLO");

    let pago = |payer: &AccountId, receiver: &AccountId, status: &str, released: u128| {
        json!({
            "operator_id": "em",
            "payer": payer,
            "receiver": receiver,
            "amount": "1000000",
            "released": released.to_string(),
            "refunded": "0",
            "status": status,
            "dispute_deadline": "0",
            "dispute": null,
        })
        .to_string()
    };
    let cargar = |pid: &'static str, raw: String| {
        let mock = e.mock.clone();
        async move {
            call_ok(
                &mock,
                mock.id(),
                "set_raw",
                json!({ "payment_id": pid, "raw": raw }),
                NearToken::from_yoctonear(0),
            )
            .await
        }
    };
    cargar("propio", pago(e.payer.id(), e.payer.id(), "Released", USDC)).await?;
    cargar("roto", "{\"payer\": ".into()).await?;
    cargar(
        "gordo",
        format!("{{\"relleno\":\"{}\"}}", "x".repeat(9 * 1024)),
    )
    .await?;
    cargar("resuelto", pago(e.payer.id(), e.worker.id(), "Resolved", 0)).await?;
    cargar("v2", pago(e.payer.id(), e.worker.id(), "Released", USDC)).await?;
    for (pid, codigo) in [
        ("propio", "E25_AUTO_CALIFICACION"),
        ("roto", "E21_RESPUESTA_INVALIDA"),
        ("gordo", "E21_RESPUESTA_INVALIDA"),
        ("resuelto", "E23_PAGO_SIN_LIBERAR"),
        ("sin-cargar", "E22_PAGO_INEXISTENTE"),
    ] {
        let m = e
            .give_medido(
                &e.payer,
                &Pedido {
                    escrow: e.mock.id(),
                    ..e.pedido(pid)
                },
            )
            .await?;
        assert_rechazo(&e, &m, codigo);
    }
    let r = e
        .give(
            &e.payer,
            &Pedido {
                escrow: e.mock.id(),
                ..e.pedido("v2")
            },
        )
        .await?;
    let out: Option<u32> = r.clone().json()?;
    assert_eq!(out, Some(0), "{:?}", r.logs());

    // El peor caso del callback: respuesta de casi 8 KiB, todo al tope y una calificada nueva.
    let mut grande: Value =
        serde_json::from_str(&pago(e.tercero.id(), e.worker.id(), "Released", u128::MAX))?;
    grande["operator_id"] = json!("o".repeat(64));
    let base = grande.to_string().len();
    grande["relleno"] = json!("x".repeat(8 * 1024 - base - 13));
    assert_eq!(grande.to_string().len(), 8 * 1024);
    cargar("grande", grande.to_string()).await?;
    let r = e
        .tercero
        .call(e.registry.id(), "give_feedback")
        .args_json(json!({
            "escrow": e.mock.id(),
            "payment_id": "grande",
            "score": 100,
            "tag1": "t".repeat(64),
            "tag2": "u".repeat(64),
            "feedback_uri": format!("https://{}", "a".repeat(504)),
            "feedback_hash": HASH,
        }))
        .deposit(NearToken::from_yoctonear(e.deposit))
        .gas(Gas::from_tgas(60))
        .transact()
        .await?;
    let out: Option<u32> = r.clone().json()?;
    assert_eq!(out, Some(1), "{:?}", r.logs());
    let cb = r
        .receipt_outcomes()
        .iter()
        .find(|o| o.logs.iter().any(|l| l.contains("feedback_given")))
        .unwrap()
        .gas_burnt;
    println!(
        "MEDIDA WR-05 peor caso (respuesta de 8 KiB, todo al tope, 60 TGas): total {:.3} TGas, callback {:.3} TGas (reservados 30)",
        tg(r.total_gas_burnt),
        tg(cb)
    );
    assert!(
        cb < Gas::from_tgas(30),
        "el callback entra en su gas estatico"
    );
    Ok(())
}

/// WR-06: dos `give_feedback` del mismo rater y pago a la vez: pasa uno solo.
#[tokio::test]
async fn wr06_dos_a_la_vez() -> anyhow::Result<()> {
    let _permiso = SANDBOXES.acquire().await?;
    let e = setup().await?;
    let id = e.released(1).await?;
    e.settle().await?;
    let b0 = e.balance(e.payer.id()).await?;
    let call = |score: u8| {
        e.payer
            .call(e.registry.id(), "give_feedback")
            .args_json(json!({ "escrow": e.arras.id(), "payment_id": id, "score": score }))
            .deposit(NearToken::from_yoctonear(e.deposit))
            .gas(Gas::from_tgas(100))
            .transact_async()
    };
    let (a, b) = tokio::join!(call(10), call(20));
    let (a, b) = (a?.await?, b?.await?);
    let ok = |r: &ExecutionFinalResult| {
        r.is_success() && r.clone().json::<Option<u32>>().unwrap().is_some()
    };
    let n_ok = [ok(&a), ok(&b)].iter().filter(|x| **x).count();
    println!(
        "MEDIDA WR-06: a {} {:?} {:?}; b {} {:?} {:?}",
        a.is_success(),
        rechazos(&a),
        a.clone()
            .into_result()
            .err()
            .map(|e| format!("{e:?}").chars().take(160).collect::<String>()),
        b.is_success(),
        rechazos(&b),
        b.clone()
            .into_result()
            .err()
            .map(|e| format!("{e:?}").chars().take(160).collect::<String>()),
    );
    assert_eq!(n_ok, 1, "pasa una sola");
    for r in [&a, &b] {
        if !ok(r) {
            let err = format!("{:?}", r.clone().into_result());
            assert!(
                err.contains("ARRAS_REG_E19")
                    || err.contains("ARRAS_REG_E18")
                    || rechazos(r) == vec!["E18_YA_CALIFICADO"],
                "{err} {:?}",
                rechazos(r)
            );
        }
    }
    let cfg = e.view("get_config", json!({})).await?;
    assert_eq!(cfg["feedbacks"], 1);
    let s = e
        .view("get_summary", json!({ "account_id": e.worker.id() }))
        .await?;
    assert_eq!(s["count"], 1);
    e.settle().await?;
    assert!(
        b0 - e.balance(e.payer.id()).await? < e.deposit,
        "un solo storage cobrado"
    );
    Ok(())
}

/// WR-07: el callback es `#[private]`: nadie de afuera lo llama (ni con un deposito ficticio que devolver).
#[tokio::test]
async fn wr07_callback_privado() -> anyhow::Result<()> {
    let _permiso = SANDBOXES.acquire().await?;
    let e = setup().await?;
    e.settle().await?;
    let f0 = e.free().await?;
    let r = e
        .tercero
        .call(e.registry.id(), "on_payment_checked")
        .args_json(json!({ "p": {
            "rater": e.tercero.id(),
            "escrow": e.arras.id(),
            "payment_id": "x",
            "score": 100,
            "tag1": null, "tag2": null, "feedback_uri": null, "feedback_hash": null,
            "deposit": "1000000000000000000000000",
            "nonce": "0",
        }}))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;
    let err = format!("{:?}", r.into_result().unwrap_err());
    assert!(
        err.contains("Method on_payment_checked is private"),
        "{err}"
    );
    e.settle().await?;
    assert!(e.free().await? >= f0, "no salio plata");
    assert_eq!(e.view("get_config", json!({})).await?["feedbacks"], 0);
    Ok(())
}

/// WR-08: revocar la propia: queda marcada, deja de contar; la segunda vez es E31; la de otro, E30.
#[tokio::test]
async fn wr08_revocar() -> anyhow::Result<()> {
    let _permiso = SANDBOXES.acquire().await?;
    let e = setup().await?;
    let id = e.released(1).await?;
    let r = e.give(&e.payer, &e.pedido(&id)).await?;
    assert_eq!(r.clone().json::<Option<u32>>()?, Some(0));
    let revocar = |who: &Account, dep: NearToken| {
        who.call(e.registry.id(), "revoke_feedback")
            .args_json(json!({ "escrow": e.arras.id(), "payment_id": id }))
            .deposit(dep)
            .transact()
    };
    let r = revocar(&e.worker, ONE_YOCTO).await?;
    assert_falla(&r, "E30");
    let r = revocar(&e.payer, NearToken::from_yoctonear(0)).await?;
    assert!(r.is_failure());
    let s0 = e.storage().await?;
    let r = revocar(&e.payer, ONE_YOCTO).await?;
    assert!(r.is_success(), "{:?}", r.clone().into_result());
    assert_eq!(e.storage().await?, s0, "revocar no cambia el storage");
    let ev = reg_events(&r, "feedback_revoked");
    assert_eq!(ev[0]["data"][0]["feedback_id"], 0);
    let f = e.view("get_feedback", json!({ "feedback_id": 0 })).await?;
    assert_eq!(f["revoked"], true);
    let s = e
        .view("get_summary", json!({ "account_id": e.worker.id() }))
        .await?;
    assert_eq!(s["count"], 0);
    assert_eq!(s["revoked"], 1);
    let r = revocar(&e.payer, ONE_YOCTO).await?;
    assert_falla(&r, "E31");
    Ok(())
}

/// WR-09: con el gas minimo justo (60 TGas) la calificacion pasa sobre v1 real.
#[tokio::test]
async fn wr09_gas_minimo() -> anyhow::Result<()> {
    let _permiso = SANDBOXES.acquire().await?;
    let e = setup().await?;
    let id = e.released(1).await?;
    let r = e
        .give(
            &e.payer,
            &Pedido {
                gas: Gas::from_tgas(60),
                ..e.pedido(&id)
            },
        )
        .await?;
    assert_eq!(r.clone().json::<Option<u32>>()?, Some(0), "{:?}", r.logs());
    let gas: Vec<String> = r
        .receipt_outcomes()
        .iter()
        .map(|o| format!("{} {:.3}", o.executor_id, tg(o.gas_burnt)))
        .collect();
    println!(
        "MEDIDA WR-09 give_feedback con 60 TGas: total {:.3} TGas; recibos: {}",
        tg(r.total_gas_burnt),
        gas.join(", ")
    );
    Ok(())
}
