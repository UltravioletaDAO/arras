//! Tests contra la wasm real en el sandbox de near-workspaces (sec. 5).
//!
//! - U-24 vive aca porque `#[private]` solo existe en el wrapper de la wasm (`near-sdk-macros`,
//!   `#[cfg(target_arch = "wasm32")]`): un test unitario que llama `Arras::new` directo no lo ve.
//! - W-01..W-12 usan el FT de prueba de near-contract-standards (`contracts/test-ft`).
//! - W-13, W-13b y W-09b usan el USDC de Circle de testnet clonado al sandbox (`import_contract`): necesitan red,
//!   asi que son opt-in (`#[ignore]`; RESPUESTA de c0der del 2026-11-01): corren con
//!   `ARRAS_TESTNET_RPC=https://rpc.testnet.fastnear.com cargo test -p arras-escrow-v1 --test workspaces -- --ignored`.
//!   Sin red, `w13_fixture_grabado` valida igual el fixture commiteado. Con `ARRAS_GRABAR_FIXTURE=1`, W-13 regraba el
//!   `EXPERIMENTAL_tx_status` del rechazo en `tests/fixtures/w13_rechazo_e06_usdc_real.json` (fixture de H-4).
//! - El sandbox (nearcore 2.13.4) se descarga una vez y queda en cache; sin red y sin cache, `NEAR_SANDBOX_BIN_PATH`.
//!
//! Las mediciones (gas de W-09, gas minimo de W-04, gas del `ft_transfer` de Circle) se imprimen con prefijo
//! `MEDIDA`: `cargo test -p arras-escrow-v1 --test workspaces -- --nocapture`.

use near_workspaces::network::Sandbox;
use near_workspaces::operations::Function;
use near_workspaces::result::ExecutionFinalResult;
use near_workspaces::types::{Gas, NearToken};
use near_workspaces::{Account, AccountDetailsPatch, AccountId, Worker};
use serde_json::{Value, json};
use tokio::sync::{OnceCell, Semaphore};

static WASM_V1: OnceCell<Vec<u8>> = OnceCell::const_new();
static WASM_FT: OnceCell<Vec<u8>> = OnceCell::const_new();
/// Cuantos sandboxes a la vez (cada test levanta el suyo).
static SANDBOXES: Semaphore = Semaphore::const_new(4);

const USDC: u128 = 1_000_000;
const ONE_YOCTO: NearToken = NearToken::from_yoctonear(1);
const DAY_NS: u64 = 86_400 * 1_000_000_000;
const USDC_TESTNET: &str = "3e2210e1184b45b64c8a434c0a7e7b23cc04ea7eb7a6c3c32520d03d4afcb8af";
const FIXTURE_W13: &str = "tests/fixtures/w13_rechazo_e06_usdc_real.json";

/// La wasm de v1, compilada una vez por corrida con cargo-near-build (`compile_project`).
async fn wasm_v1() -> anyhow::Result<&'static [u8]> {
    let w = WASM_V1
        .get_or_try_init(|| async { near_workspaces::compile_project("./").await })
        .await?;
    Ok(w.as_slice())
}

async fn wasm_ft() -> anyhow::Result<&'static [u8]> {
    let w = WASM_FT
        .get_or_try_init(|| async { near_workspaces::compile_project("../test-ft").await })
        .await?;
    Ok(w.as_slice())
}

#[derive(Clone)]
struct Opts {
    fees_en_ft: bool,
    worker_en_ft: bool,
    payer_nep145: bool,
    tope_por_pago: u128,
    tope_tvl_por_payer: u128,
    sponsor_max: u128,
}

impl Default for Opts {
    fn default() -> Self {
        Self {
            fees_en_ft: true,
            worker_en_ft: true,
            payer_nep145: true,
            tope_por_pago: 10 * USDC,
            tope_tvl_por_payer: 20 * USDC,
            sponsor_max: NearToken::from_millinear(20).as_yoctonear(),
        }
    }
}

struct Env {
    w: Worker<Sandbox>,
    root: Account,
    ft: Account,
    arras: Account,
    owner: Account,
    capturer: Account,
    fees: Account,
    payer: Account,
    worker: Account,
    now: u64,
}

async fn sub(root: &Account, name: &str, near: u128) -> anyhow::Result<Account> {
    Ok(root
        .create_subaccount(name)
        .initial_balance(NearToken::from_near(near))
        .transact()
        .await?
        .into_result()?)
}

/// Despliega Arras en una cuenta nueva con deploy + `new` en la misma transaccion (como `with-init-call`).
async fn deploy_arras(
    root: &Account,
    name: &str,
    owner: &AccountId,
    guardian: &AccountId,
    token: &AccountId,
    tope_por_pago: u128,
) -> anyhow::Result<Account> {
    let acc = sub(root, name, 10).await?;
    let r = acc
        .batch(acc.id())
        .deploy(wasm_v1().await?)
        .call(
            Function::new("new")
                .args_json(json!({
                    "owner": owner,
                    "guardian": guardian,
                    "token": token,
                    "tope_por_pago": tope_por_pago.to_string(),
                    "tope_tvl_global": (1_000 * USDC).to_string(),
                }))
                .gas(Gas::from_tgas(30)),
        )
        .transact()
        .await?;
    assert!(r.is_success(), "deploy + new: {:?}", r.into_result());
    Ok(acc)
}

async fn ft_register(root: &Account, ft: &AccountId, who: &AccountId) -> anyhow::Result<()> {
    let r = root
        .call(ft, "storage_deposit")
        .args_json(json!({ "account_id": who, "registration_only": true }))
        .deposit(NearToken::from_millinear(13))
        .transact()
        .await?;
    assert!(
        r.is_success(),
        "storage_deposit en el FT: {:?}",
        r.into_result()
    );
    Ok(())
}

async fn ft_send(
    from: &Account,
    ft: &AccountId,
    to: &AccountId,
    amount: u128,
) -> anyhow::Result<()> {
    let r = from
        .call(ft, "ft_transfer")
        .args_json(json!({ "receiver_id": to, "amount": amount.to_string() }))
        .deposit(ONE_YOCTO)
        .transact()
        .await?;
    assert!(r.is_success(), "ft_transfer: {:?}", r.into_result());
    Ok(())
}

fn operator_input(owner: &AccountId, capturer: &AccountId, fees: &AccountId, o: &Opts) -> Value {
    json!({
        "owner": owner,
        "capturers": [capturer],
        "fee_bps": 1300,
        "fee_recipient": fees,
        "min_amount": "10000",
        "tope_tvl_operador": (200 * USDC).to_string(),
        "tope_tvl_por_payer": o.tope_tvl_por_payer.to_string(),
        "sponsor_max_por_payer": o.sponsor_max.to_string(),
    })
}

/// Sandbox con el FT de prueba, Arras v1 y el operador `em` de la sec. 8.
async fn setup(o: Opts) -> anyhow::Result<Env> {
    let w = near_workspaces::sandbox().await?;
    let root = w.root_account()?;
    let ft = sub(&root, "ft", 20).await?;
    let r = ft.deploy(wasm_ft().await?).await?;
    assert!(r.is_success());
    let r = ft
        .call(ft.id(), "new")
        .args_json(json!({ "owner_id": ft.id(), "total_supply": (1_000_000 * USDC).to_string() }))
        .transact()
        .await?;
    assert!(r.is_success(), "{:?}", r.into_result());
    let owner = sub(&root, "owner", 10).await?;
    let guardian = sub(&root, "guardian", 5).await?;
    let capturer = sub(&root, "capturer", 10).await?;
    let fees = sub(&root, "fees", 5).await?;
    let payer = sub(&root, "payer", 10).await?;
    let worker = sub(&root, "worker", 5).await?;
    let arras = deploy_arras(
        &root,
        "arras",
        owner.id(),
        guardian.id(),
        ft.id(),
        o.tope_por_pago,
    )
    .await?;
    for (who, si) in [
        (arras.id(), true),
        (payer.id(), true),
        (worker.id(), o.worker_en_ft),
        (fees.id(), o.fees_en_ft),
    ] {
        if si {
            ft_register(&root, ft.id(), who).await?;
        }
    }
    ft_send(&ft, ft.id(), payer.id(), 100 * USDC).await?;
    let r = owner
        .call(arras.id(), "register_operator")
        .args_json(json!({ "operator_id": "em", "input": operator_input(owner.id(), capturer.id(), fees.id(), &o) }))
        .deposit(ONE_YOCTO)
        .transact()
        .await?;
    assert!(r.is_success(), "register_operator: {:?}", r.into_result());
    if o.payer_nep145 {
        nep145(&payer, arras.id(), NearToken::from_millinear(50)).await?;
    }
    let now = w.view_block().await?.timestamp();
    Ok(Env {
        w,
        root,
        ft,
        arras,
        owner,
        capturer,
        fees,
        payer,
        worker,
        now,
    })
}

async fn nep145(who: &Account, arras: &AccountId, amount: NearToken) -> anyhow::Result<()> {
    let r = who
        .call(arras, "storage_deposit")
        .args_json(json!({}))
        .deposit(amount)
        .transact()
        .await?;
    assert!(
        r.is_success(),
        "storage_deposit en Arras: {:?}",
        r.into_result()
    );
    Ok(())
}

impl Env {
    /// `msg` canonico con plazos de 1 y 2 dias desde el inicio del test.
    fn msg(&self, receiver: &AccountId, salt: u8) -> String {
        msg_for(receiver, self.now + DAY_NS, self.now + 2 * DAY_NS, salt)
    }

    async fn authorize(
        &self,
        payer: &Account,
        amount: u128,
        msg: &str,
    ) -> anyhow::Result<ExecutionFinalResult> {
        self.authorize_gas(payer, amount, msg, Gas::from_tgas(100))
            .await
    }

    async fn authorize_gas(
        &self,
        payer: &Account,
        amount: u128,
        msg: &str,
        gas: Gas,
    ) -> anyhow::Result<ExecutionFinalResult> {
        Ok(payer
            .call(self.ft.id(), "ft_transfer_call")
            .args_json(
                json!({ "receiver_id": self.arras.id(), "amount": amount.to_string(), "msg": msg }),
            )
            .deposit(ONE_YOCTO)
            .gas(gas)
            .transact()
            .await?)
    }

    /// authorize que tiene que pasar: el token usa todo el monto. Devuelve el `payment_id`.
    async fn authorize_ok(
        &self,
        payer: &Account,
        amount: u128,
        msg: &str,
    ) -> anyhow::Result<String> {
        let r = self.authorize(payer, amount, msg).await?;
        let used: String = r.clone().json()?;
        assert_eq!(
            used,
            amount.to_string(),
            "authorize rechazado: {:?}",
            r.logs()
        );
        self.payment_id(payer.id(), amount, msg).await
    }

    async fn payment_id(
        &self,
        payer: &AccountId,
        amount: u128,
        msg: &str,
    ) -> anyhow::Result<String> {
        let id: String = self
            .view(
                "compute_payment_id",
                json!({ "sender_id": payer, "amount": amount.to_string(), "msg": msg }),
            )
            .await?;
        Ok(id)
    }

    async fn view<T: near_sdk::serde::de::DeserializeOwned>(
        &self,
        method: &str,
        args: Value,
    ) -> anyhow::Result<T> {
        Ok(self
            .root
            .view(self.arras.id(), method)
            .args_json(args)
            .await?
            .json()?)
    }

    async fn ft_balance(&self, who: &AccountId) -> anyhow::Result<u128> {
        let b: String = self
            .root
            .view(self.ft.id(), "ft_balance_of")
            .args_json(json!({ "account_id": who }))
            .await?
            .json()?;
        Ok(b.parse()?)
    }

    async fn claimable(&self, who: &AccountId) -> anyhow::Result<u128> {
        let c: String = self
            .view("get_claimable", json!({ "account": who }))
            .await?;
        Ok(c.parse()?)
    }

    async fn config(&self) -> anyhow::Result<Value> {
        self.view("get_config", json!({})).await
    }

    async fn release(&self, id: &str, amount: Option<u128>) -> anyhow::Result<()> {
        let r = self
            .capturer
            .call(self.arras.id(), "release")
            .args_json(json!({ "payment_id": id, "amount": amount.map(|a| a.to_string()) }))
            .transact()
            .await?;
        assert!(r.is_success(), "release: {:?}", r.into_result());
        Ok(())
    }

    async fn withdraw_for(
        &self,
        by: &Account,
        who: &AccountId,
        gas: Gas,
    ) -> anyhow::Result<ExecutionFinalResult> {
        Ok(by
            .call(self.arras.id(), "withdraw_for")
            .args_json(json!({ "account": who }))
            .gas(gas)
            .transact()
            .await?)
    }

    /// `sum(vivos) + sum(claimable) + in_flight == tvl_global <= ft_balance_of(contrato)` (sec. 3.5).
    async fn invariante(&self, cuentas: &[&AccountId]) -> anyhow::Result<()> {
        let cfg = self.config().await?;
        let tvl: u128 = cfg["tvl_global"].as_str().unwrap().parse()?;
        let in_flight: u128 = cfg["in_flight"].as_str().unwrap().parse()?;
        let pagos: Vec<(String, Value)> = self
            .view(
                "list_payments",
                json!({ "operator_id": "em", "from_index": 0, "limit": 50 }),
            )
            .await?;
        let mut vivos = 0u128;
        for (_, p) in &pagos {
            let st = p["status"].as_str().unwrap();
            if st == "Authorized" || st == "PartiallyReleased" {
                let n = |k: &str| p[k].as_str().unwrap().parse::<u128>().unwrap();
                vivos += n("amount") - n("released") - n("refunded");
            }
        }
        let mut claim = 0u128;
        for c in cuentas {
            claim += self.claimable(c).await?;
        }
        assert_eq!(
            vivos + claim + in_flight,
            tvl,
            "sum(vivos) + sum(claimable) + in_flight == tvl_global"
        );
        assert!(
            tvl <= self.ft_balance(self.arras.id()).await?,
            "tvl_global <= ft_balance_of(contrato)"
        );
        Ok(())
    }

    fn todas(&self) -> [&AccountId; 3] {
        [self.payer.id(), self.worker.id(), self.fees.id()]
    }
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

/// Los eventos `EVENT_JSON` de un estandar y nombre dados, en orden.
fn events(r: &ExecutionFinalResult, standard: &str, event: &str) -> Vec<Value> {
    r.logs()
        .iter()
        .filter_map(|l| l.strip_prefix("EVENT_JSON:"))
        .filter_map(|j| serde_json::from_str::<Value>(j).ok())
        .filter(|e| e["standard"] == standard && e["event"] == event)
        .collect()
}

fn withdrawn_ok(r: &ExecutionFinalResult) -> Vec<bool> {
    events(r, "arras", "withdrawn")
        .iter()
        .map(|e| e["data"][0]["ok"].as_bool().unwrap())
        .collect()
}

/// U-24 `new_ajeno`: `new` llamado por otra cuenta -> panic (`#[private]`); la propia cuenta si inicializa.
#[tokio::test]
async fn u24_new_ajeno() -> anyhow::Result<()> {
    let _permiso = SANDBOXES.acquire().await?;
    let worker = near_workspaces::sandbox().await?;
    let contract = worker.dev_deploy(wasm_v1().await?).await?;
    let ajeno = worker.dev_create_account().await?;
    let args = json!({
        "owner": ajeno.id(),
        "guardian": ajeno.id(),
        "token": ajeno.id(),
        "tope_por_pago": "10000000",
        "tope_tvl_global": "1000000000",
    });
    let r = ajeno
        .call(contract.id(), "new")
        .args_json(&args)
        .transact()
        .await?;
    assert!(r.is_failure(), "new ajeno no puede inicializar");
    let err = format!("{:?}", r.into_result().unwrap_err());
    assert!(err.contains("Method new is private"), "{err}");
    let cfg = contract.view("get_config").await;
    assert!(cfg.is_err(), "sin init el contrato sigue sin estado");

    let ok = contract.call("new").args_json(&args).transact().await?;
    assert!(ok.is_success(), "{:?}", ok.into_result());
    let cfg: Value = contract.view("get_config").await?.json()?;
    assert_eq!(cfg["version"], "arras-v1");
    // Una segunda inicializacion falla (estado ya existe).
    let again = contract.call("new").args_json(&args).transact().await?;
    assert!(again.is_failure());
    Ok(())
}

/// `on_withdraw` es `#[private]` y `ft_on_transfer` de una cuenta que no es el token devuelve todo con `E01`.
#[tokio::test]
async fn callbacks_ajenos() -> anyhow::Result<()> {
    let _permiso = SANDBOXES.acquire().await?;
    let e = setup(Opts::default()).await?;
    let r = e
        .payer
        .call(e.arras.id(), "on_withdraw")
        .args_json(json!({ "account": e.payer.id(), "amount": "1000" }))
        .transact()
        .await?;
    let err = format!("{:?}", r.into_result().unwrap_err());
    assert!(err.contains("Method on_withdraw is private"), "{err}");
    assert_eq!(e.claimable(e.payer.id()).await?, 0);
    let r = e
        .payer
        .call(e.arras.id(), "ft_on_transfer")
        .args_json(json!({ "sender_id": e.payer.id(), "amount": "1000000", "msg": e.msg(e.worker.id(), 0) }))
        .transact()
        .await?;
    let back: String = r.clone().json()?;
    assert_eq!(back, "1000000");
    assert!(
        r.logs().contains(&"arras:rechazo:E01_TOKEN_NO_PERMITIDO"),
        "{:?}",
        r.logs()
    );
    assert_eq!(e.config().await?["payments"], 0);
    Ok(())
}

/// W-01 `flujo_feliz`: saldos exactos; contrato en 0; `tvl_global == 0`.
#[tokio::test]
async fn w01_flujo_feliz() -> anyhow::Result<()> {
    let _permiso = SANDBOXES.acquire().await?;
    let e = setup(Opts::default()).await?;
    let id = e
        .authorize_ok(&e.payer, USDC, &e.msg(e.worker.id(), 1))
        .await?;
    assert_eq!(e.ft_balance(e.arras.id()).await?, USDC);
    assert_eq!(e.ft_balance(e.payer.id()).await?, 99 * USDC);
    e.invariante(&e.todas()).await?;
    e.release(&id, None).await?;
    e.invariante(&e.todas()).await?;
    for who in [e.worker.id(), e.fees.id()] {
        let r = e.withdraw_for(&e.capturer, who, Gas::from_tgas(50)).await?;
        assert!(r.is_success(), "{:?}", r.into_result());
        assert_eq!(withdrawn_ok(&r), vec![true]);
    }
    assert_eq!(e.ft_balance(e.worker.id()).await?, 870_000);
    assert_eq!(e.ft_balance(e.fees.id()).await?, 130_000);
    assert_eq!(e.ft_balance(e.payer.id()).await?, 99 * USDC);
    assert_eq!(e.ft_balance(e.arras.id()).await?, 0);
    let cfg = e.config().await?;
    assert_eq!(
        (cfg["tvl_global"].as_str(), cfg["in_flight"].as_str()),
        (Some("0"), Some("0"))
    );
    e.invariante(&e.todas()).await?;
    Ok(())
}

/// W-02 `refund_y_withdraw_payer`: 100 % al payer.
#[tokio::test]
async fn w02_refund_y_withdraw_payer() -> anyhow::Result<()> {
    let _permiso = SANDBOXES.acquire().await?;
    let e = setup(Opts::default()).await?;
    let id = e
        .authorize_ok(&e.payer, 3 * USDC, &e.msg(e.worker.id(), 1))
        .await?;
    let r = e
        .capturer
        .call(e.arras.id(), "refund")
        .args_json(json!({ "payment_id": id }))
        .transact()
        .await?;
    assert!(r.is_success(), "{:?}", r.into_result());
    let r = e
        .payer
        .call(e.arras.id(), "withdraw")
        .gas(Gas::from_tgas(50))
        .transact()
        .await?;
    assert!(r.is_success(), "{:?}", r.into_result());
    assert_eq!(withdrawn_ok(&r), vec![true]);
    assert_eq!(e.ft_balance(e.payer.id()).await?, 100 * USDC);
    assert_eq!(e.ft_balance(e.arras.id()).await?, 0);
    assert_eq!(e.config().await?["tvl_global"], "0");
    e.invariante(&e.todas()).await?;
    Ok(())
}

/// W-03 `fee_recipient_sin_registrar`: solo esa pata vuelve a `claimable`.
#[tokio::test]
async fn w03_fee_recipient_sin_registrar() -> anyhow::Result<()> {
    let _permiso = SANDBOXES.acquire().await?;
    let e = setup(Opts {
        fees_en_ft: false,
        ..Default::default()
    })
    .await?;
    let id = e
        .authorize_ok(&e.payer, USDC, &e.msg(e.worker.id(), 1))
        .await?;
    e.release(&id, None).await?;
    let r = e
        .withdraw_for(&e.capturer, e.worker.id(), Gas::from_tgas(50))
        .await?;
    assert_eq!(withdrawn_ok(&r), vec![true]);
    let r = e
        .withdraw_for(&e.capturer, e.fees.id(), Gas::from_tgas(50))
        .await?;
    assert!(
        r.is_success(),
        "el callback no falla: {:?}",
        r.clone().into_result()
    );
    assert_eq!(withdrawn_ok(&r), vec![false]);
    let back: String = r.clone().json()?;
    assert_eq!(back, "0", "on_withdraw devuelve lo transferido");
    assert!(
        !r.receipt_failures().is_empty(),
        "el ft_transfer al no registrado falla"
    );
    assert_eq!(e.ft_balance(e.worker.id()).await?, 870_000);
    assert_eq!(e.claimable(e.fees.id()).await?, 130_000);
    assert_eq!(e.ft_balance(e.arras.id()).await?, 130_000);
    let cfg = e.config().await?;
    assert_eq!(
        (cfg["tvl_global"].as_str(), cfg["in_flight"].as_str()),
        (Some("130000"), Some("0"))
    );
    e.invariante(&e.todas()).await?;
    Ok(())
}

/// W-04 `callback_gas_minimo`: con cualquier gas, o la llamada entera falla (nada cambia) o el callback corre y no
/// se pierde nada; se anota el gas minimo para la pata buena y para la que falla.
#[tokio::test]
async fn w04_callback_gas_minimo() -> anyhow::Result<()> {
    let _permiso = SANDBOXES.acquire().await?;
    let e = setup(Opts {
        fees_en_ft: false,
        ..Default::default()
    })
    .await?;
    let id = e
        .authorize_ok(&e.payer, USDC, &e.msg(e.worker.id(), 1))
        .await?;
    e.release(&id, None).await?;
    let mut minimos = vec![];
    for (who, esperado, registrado) in [
        (e.worker.id(), 870_000u128, true),
        (e.fees.id(), 130_000u128, false),
    ] {
        let mut minimo = None;
        for tgas in (10..=40).step_by(1) {
            let r = e
                .withdraw_for(&e.capturer, who, Gas::from_tgas(tgas))
                .await?;
            let cfg = e.config().await?;
            assert_eq!(cfg["in_flight"], "0", "nada queda en vuelo con {tgas} TGas");
            let claim = e.claimable(who).await?;
            let recibido = e.ft_balance(who).await.unwrap_or(0);
            if events(&r, "arras", "withdrawn").is_empty() {
                assert!(
                    r.is_failure(),
                    "sin callback la llamada entera falla ({tgas} TGas)"
                );
                assert_eq!(
                    (claim, recibido),
                    (esperado, 0),
                    "nada cambio con {tgas} TGas"
                );
                continue;
            }
            assert!(r.is_success(), "{tgas} TGas: {:?}", r.into_result());
            if registrado {
                assert_eq!(withdrawn_ok(&r), vec![true]);
                assert_eq!(
                    (claim, recibido),
                    (0, esperado),
                    "cobro completo con {tgas} TGas"
                );
            } else {
                assert_eq!(withdrawn_ok(&r), vec![false]);
                assert_eq!(claim, esperado, "restaurado con {tgas} TGas");
            }
            minimo = Some(tgas);
            break;
        }
        let minimo = minimo.expect("algun gas alcanza");
        minimos.push(minimo);
        e.invariante(&e.todas()).await?;
    }
    println!(
        "MEDIDA W-04 gas minimo de withdraw_for: {} TGas (ft_transfer ok), {} TGas (ft_transfer falla)",
        minimos[0], minimos[1]
    );
    Ok(())
}

/// W-05 `dos_withdraw_en_vuelo`: sin candado, dos retiros a la vez: uno cobra y el otro da `E30`; despues, uno nuevo
/// cobra lo nuevo. La invariante no se rompe.
#[tokio::test]
async fn w05_dos_withdraw_en_vuelo() -> anyhow::Result<()> {
    let _permiso = SANDBOXES.acquire().await?;
    let e = setup(Opts::default()).await?;
    let id = e
        .authorize_ok(&e.payer, USDC, &e.msg(e.worker.id(), 1))
        .await?;
    e.release(&id, None).await?;
    let a = e
        .worker
        .call(e.arras.id(), "withdraw")
        .gas(Gas::from_tgas(50))
        .transact_async()
        .await?;
    let b = e
        .capturer
        .call(e.arras.id(), "withdraw_for")
        .args_json(json!({ "account": e.worker.id() }))
        .gas(Gas::from_tgas(50))
        .transact_async()
        .await?;
    let (ra, rb) = (a.await?, b.await?);
    let oks = [ra.is_success(), rb.is_success()];
    assert_eq!(
        oks.iter().filter(|x| **x).count(),
        1,
        "exactamente uno cobra: {oks:?}"
    );
    let fallido = if ra.is_success() { rb } else { ra };
    assert!(format!("{:?}", fallido.into_result().unwrap_err()).contains("ARRAS_E30"));
    assert_eq!(
        e.ft_balance(e.worker.id()).await?,
        870_000,
        "sin doble pago"
    );
    e.invariante(&e.todas()).await?;
    let id2 = e
        .authorize_ok(&e.payer, USDC, &e.msg(e.worker.id(), 2))
        .await?;
    e.release(&id2, None).await?;
    let r = e
        .worker
        .call(e.arras.id(), "withdraw")
        .gas(Gas::from_tgas(50))
        .transact()
        .await?;
    assert_eq!(withdrawn_ok(&r), vec![true]);
    assert_eq!(e.ft_balance(e.worker.id()).await?, 2 * 870_000);
    e.invariante(&e.todas()).await?;
    Ok(())
}

/// W-06 `griefing_storage`: pagos de `min_amount` desde un payer sin NEP-145 agotan su `sponsor_max_por_payer` ->
/// `E14`; otros payers siguen. `tope_tvl_por_payer` alto para que `E08` no salte antes.
#[tokio::test]
async fn w06_griefing_storage() -> anyhow::Result<()> {
    let _permiso = SANDBOXES.acquire().await?;
    let e = setup(Opts {
        tope_tvl_por_payer: 20 * USDC,
        ..Default::default()
    })
    .await?;
    let r = e
        .owner
        .call(e.arras.id(), "fund_operator_storage")
        .args_json(json!({ "operator_id": "em" }))
        .deposit(NearToken::from_millinear(500))
        .transact()
        .await?;
    assert!(r.is_success());
    let atacante = sub(&e.root, "atacante", 5).await?;
    let honesto = sub(&e.root, "honesto", 5).await?;
    for a in [&atacante, &honesto] {
        ft_register(&e.root, e.ft.id(), a.id()).await?;
        ft_send(&e.ft, e.ft.id(), a.id(), 10 * USDC).await?;
    }
    let mut patrocinados = 0u32;
    let mut cobrado = 0u128;
    for salt in 0..50u8 {
        let msg = e.msg(e.worker.id(), salt);
        let r = e.authorize(&atacante, 10_000, &msg).await?;
        let used: String = r.clone().json()?;
        if used == "0" {
            assert!(
                r.logs().contains(&"arras:rechazo:E14_SIN_STORAGE"),
                "{:?}",
                r.logs()
            );
            break;
        }
        let p: Value = e
            .view(
                "get_payment",
                json!({ "payment_id": e.payment_id(atacante.id(), 10_000, &msg).await? }),
            )
            .await?;
        assert_eq!(p["sponsored"], true);
        cobrado += p["storage_charged"].as_str().unwrap().parse::<u128>()?;
        patrocinados += 1;
    }
    assert!(
        (1..50).contains(&patrocinados),
        "E14 corta el patrocinio ({patrocinados} pagos)"
    );
    assert!(cobrado <= NearToken::from_millinear(20).as_yoctonear());
    assert_eq!(
        e.ft_balance(atacante.id()).await?,
        10 * USDC - u128::from(patrocinados) * 10_000,
        "el rechazo devolvio todo"
    );
    // Otro payer sin NEP-145 todavia tiene su cupo; el payer con NEP-145 no depende del pool.
    e.authorize_ok(&honesto, 10_000, &e.msg(e.worker.id(), 1))
        .await?;
    e.authorize_ok(&e.payer, 10_000, &e.msg(e.worker.id(), 1))
        .await?;
    println!(
        "MEDIDA W-06 pagos patrocinados antes de E14: {patrocinados} (cobrado {cobrado} yocto de 0,02 NEAR)"
    );
    Ok(())
}

/// W-07 `griefing_tvl`: un payer llena su `tope_tvl_por_payer` -> `E08`; otros siguen.
#[tokio::test]
async fn w07_griefing_tvl() -> anyhow::Result<()> {
    let _permiso = SANDBOXES.acquire().await?;
    let e = setup(Opts::default()).await?;
    e.authorize_ok(&e.payer, 10 * USDC, &e.msg(e.worker.id(), 1))
        .await?;
    e.authorize_ok(&e.payer, 10 * USDC, &e.msg(e.worker.id(), 2))
        .await?;
    let r = e
        .authorize(&e.payer, 10_000, &e.msg(e.worker.id(), 3))
        .await?;
    let used: String = r.clone().json()?;
    assert_eq!(used, "0");
    assert!(
        r.logs().contains(&"arras:rechazo:E08_TVL_PAYER"),
        "{:?}",
        r.logs()
    );
    assert_eq!(e.ft_balance(e.payer.id()).await?, 80 * USDC);
    let otro = sub(&e.root, "otro", 5).await?;
    ft_register(&e.root, e.ft.id(), otro.id()).await?;
    ft_send(&e.ft, e.ft.id(), otro.id(), 10 * USDC).await?;
    nep145(&otro, e.arras.id(), NearToken::from_millinear(50)).await?;
    e.authorize_ok(&otro, 10 * USDC, &e.msg(e.worker.id(), 3))
        .await?;
    e.invariante(&e.todas()).await?;
    Ok(())
}

/// W-08 `sin_llaves`: tras deploy + `delete-keys`, `view_access_key_list` vacio; el contrato sigue respondiendo.
#[tokio::test]
async fn w08_sin_llaves() -> anyhow::Result<()> {
    let _permiso = SANDBOXES.acquire().await?;
    let w = near_workspaces::sandbox().await?;
    let root = w.root_account()?;
    let owner = sub(&root, "owner", 5).await?;
    let arras = deploy_arras(&root, "v1", owner.id(), owner.id(), root.id(), 10 * USDC).await?;
    let keys = w.view_access_keys(arras.id()).await?;
    assert_eq!(keys.len(), 1);
    let r = arras
        .batch(arras.id())
        .delete_key(keys[0].public_key.clone())
        .transact()
        .await?;
    assert!(r.is_success(), "{:?}", r.into_result());
    let keys = w.view_access_keys(arras.id()).await?;
    assert!(keys.is_empty(), "list-keys vacio: {keys:?}");
    // La cuenta ya no firma nada (ni un redeploy).
    let r = arras
        .batch(arras.id())
        .deploy(wasm_v1().await?)
        .transact()
        .await;
    assert!(
        r.is_err() || r.unwrap().is_failure(),
        "sin llaves no hay redeploy"
    );
    let cfg: Value = root.view(arras.id(), "get_config").await?.json()?;
    assert_eq!(cfg["version"], "arras-v1");
    let r = owner
        .call(arras.id(), "pause")
        .deposit(ONE_YOCTO)
        .transact()
        .await?;
    assert!(
        r.is_success(),
        "sigue aceptando llamadas: {:?}",
        r.into_result()
    );
    Ok(())
}

/// W-09 `gas_ft_transfer_call`: gas real del authorize (lo usa H-4 para `NEAR_ESCROW_*_GAS_TGAS`).
#[tokio::test]
async fn w09_gas_ft_transfer_call() -> anyhow::Result<()> {
    let _permiso = SANDBOXES.acquire().await?;
    let e = setup(Opts::default()).await?;
    let msg = e.msg(e.worker.id(), 1);
    let r = e.authorize(&e.payer, USDC, &msg).await?;
    let used: String = r.clone().json()?;
    assert_eq!(used, USDC.to_string());
    let mut detalle = vec![];
    for o in r.receipt_outcomes() {
        detalle.push(format!(
            "{}={:.3}",
            o.executor_id,
            o.gas_burnt.as_gas() as f64 / 1e12
        ));
    }
    let total = r.total_gas_burnt.as_gas() as f64 / 1e12;
    // El gas prepagado minimo con el que el authorize pasa (busqueda binaria, pagos nuevos en cada intento).
    let (mut lo, mut hi) = (10u64, 100u64);
    let mut salt = 2u8;
    while hi - lo > 1 {
        let mid = (lo + hi) / 2;
        let m = e.msg(e.worker.id(), salt);
        salt += 1;
        let r = e
            .authorize_gas(&e.payer, 10_000, &m, Gas::from_tgas(mid))
            .await?;
        let ok = r.is_success()
            && r.clone()
                .json::<String>()
                .map(|u| u == "10000")
                .unwrap_or(false);
        if ok { hi = mid } else { lo = mid }
    }
    println!(
        "MEDIDA W-09 ft_transfer_call authorize: gas quemado total {total:.3} TGas; por receipt [{}]; prepagado minimo que pasa {hi} TGas",
        detalle.join(", ")
    );
    assert!(total < 100.0);
    e.invariante(&e.todas()).await?;
    Ok(())
}

/// W-10 `withdraw_dos_veces`: release -> withdraw OK -> release -> withdraw OK: el segundo cobra.
#[tokio::test]
async fn w10_withdraw_dos_veces() -> anyhow::Result<()> {
    let _permiso = SANDBOXES.acquire().await?;
    let e = setup(Opts::default()).await?;
    for (salt, total) in [(1u8, 870_000u128), (2, 1_740_000)] {
        let id = e
            .authorize_ok(&e.payer, USDC, &e.msg(e.worker.id(), salt))
            .await?;
        e.release(&id, None).await?;
        let r = e
            .worker
            .call(e.arras.id(), "withdraw")
            .gas(Gas::from_tgas(50))
            .transact()
            .await?;
        assert!(r.is_success(), "{:?}", r.into_result());
        assert_eq!(withdrawn_ok(&r), vec![true]);
        assert_eq!(e.ft_balance(e.worker.id()).await?, total);
    }
    e.invariante(&e.todas()).await?;
    Ok(())
}

/// W-11 `reserva_al_minimo`: saldo libre del contrato al minimo + `on_withdraw(Err)`: sin panic (la entrada de
/// `claimable` existe en 0, no hacen falta bytes nuevos).
#[tokio::test]
async fn w11_reserva_al_minimo() -> anyhow::Result<()> {
    let _permiso = SANDBOXES.acquire().await?;
    let e = setup(Opts {
        fees_en_ft: false,
        ..Default::default()
    })
    .await?;
    let id = e
        .authorize_ok(&e.payer, USDC, &e.msg(e.worker.id(), 1))
        .await?;
    e.release(&id, None).await?;
    let h: Value = e.view("get_storage_health", json!({})).await?;
    let n = |k: &str| h[k].as_str().unwrap().parse::<u128>().unwrap();
    let account = e.w.view_account(e.arras.id()).await?;
    let ocupado = u128::from(account.storage_usage) * 10u128.pow(19);
    // Saldo = storage ocupado + lo que es de los payers y del pool + 1 yocto (el del ft_transfer).
    let minimo = ocupado + n("nep145_available") + n("sponsor_pools") + 1;
    e.w.patch(e.arras.id())
        .account_from_current(move |actual| {
            AccountDetailsPatch::from(actual).balance(NearToken::from_yoctonear(minimo))
        })
        .transact()
        .await?;
    let h: Value = e.view("get_storage_health", json!({})).await?;
    assert_eq!(h["ok"], false, "la alarma salta: {h}");
    let r = e
        .withdraw_for(&e.capturer, e.fees.id(), Gas::from_tgas(50))
        .await?;
    assert!(
        r.is_success(),
        "on_withdraw(Err) sin panic: {:?}",
        r.clone().into_result()
    );
    assert_eq!(withdrawn_ok(&r), vec![false]);
    let arras_fallas: Vec<_> = r
        .receipt_failures()
        .into_iter()
        .filter(|o| &o.executor_id == e.arras.id())
        .collect();
    assert!(
        arras_fallas.is_empty(),
        "ningun receipt de Arras falla: {arras_fallas:?}"
    );
    assert_eq!(e.claimable(e.fees.id()).await?, 130_000);
    e.invariante(&e.todas()).await?;
    Ok(())
}

/// W-12 `worker_sin_registro_usdc`: `withdraw_for` -> `ok:false`, saldo intacto en `claimable`; tras
/// `storage_deposit` en el token, cobra.
#[tokio::test]
async fn w12_worker_sin_registro_usdc() -> anyhow::Result<()> {
    let _permiso = SANDBOXES.acquire().await?;
    let e = setup(Opts {
        worker_en_ft: false,
        ..Default::default()
    })
    .await?;
    let id = e
        .authorize_ok(&e.payer, USDC, &e.msg(e.worker.id(), 1))
        .await?;
    e.release(&id, None).await?;
    let r = e
        .withdraw_for(&e.capturer, e.worker.id(), Gas::from_tgas(50))
        .await?;
    assert_eq!(withdrawn_ok(&r), vec![false]);
    assert_eq!(e.claimable(e.worker.id()).await?, 870_000);
    e.invariante(&e.todas()).await?;
    ft_register(&e.root, e.ft.id(), e.worker.id()).await?;
    let r = e
        .withdraw_for(&e.capturer, e.worker.id(), Gas::from_tgas(50))
        .await?;
    assert_eq!(withdrawn_ok(&r), vec![true]);
    assert_eq!(e.ft_balance(e.worker.id()).await?, 870_000);
    assert_eq!(e.claimable(e.worker.id()).await?, 0);
    e.invariante(&e.todas()).await?;
    Ok(())
}

/// RPC de testnet para los tests con red (opt-in): `ARRAS_TESTNET_RPC`, o FastNEAR si se corren con `--ignored`
/// sin la variable.
fn testnet_rpc() -> String {
    std::env::var("ARRAS_TESTNET_RPC")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "https://rpc.testnet.fastnear.com".into())
}

/// El USDC de Circle de testnet clonado al sandbox, con un minter configurado por el flujo de multisig del propio
/// contrato (dos master minters, dos controllers del mismo minter) y 100 USDC acunados al payer.
struct EnvUsdc {
    e: Env,
}

async fn setup_usdc_real() -> anyhow::Result<EnvUsdc> {
    let w = near_workspaces::sandbox().await?;
    let rpc = testnet_rpc();
    let testnet = near_workspaces::testnet().rpc_addr(&rpc).await?;
    let root = w.root_account()?;
    let usdc_testnet: AccountId = USDC_TESTNET.parse()?;
    let dest: AccountId = format!("usdc.{}", root.id()).parse()?;
    let usdc = w
        .import_contract(&usdc_testnet, &testnet)
        .dest_account_id(&dest)
        .initial_balance(NearToken::from_near(50))
        .transact()
        .await?;
    let m1 = sub(&root, "minter1", 10).await?;
    let m2 = sub(&root, "minter2", 10).await?;
    let meta = json!({"spec":"ft-1.0.0","name":"USD Coin","symbol":"USDC","icon":null,"reference":null,"reference_hash":null,"decimals":6});
    let r = usdc
        .call("init")
        .args_json(json!({
            "admin_ids": [m1.id()], "master_minter_ids": [m1.id(), m2.id()], "owner_ids": [m1.id()],
            "pauser_ids": [m1.id()], "blocklister_id": m1.id(), "metadata": meta,
        }))
        .max_gas()
        .transact()
        .await?;
    assert!(r.is_success(), "init de Circle: {:?}", r.into_result());
    let pasos = [
        (
            &m1,
            "create_multisig_request",
            json!({"action": {"ConfigureController": {"controller_id": m1.id(), "minter_id": m1.id()}}}),
        ),
        (
            &m1,
            "create_multisig_request",
            json!({"action": {"ConfigureController": {"controller_id": m2.id(), "minter_id": m1.id()}}}),
        ),
        (&m1, "approve_multisig_request", json!({"request_id": 0})),
        (&m2, "approve_multisig_request", json!({"request_id": 0})),
        (&m1, "execute_multisig_request", json!({"request_id": 0})),
        (&m1, "approve_multisig_request", json!({"request_id": 1})),
        (&m2, "approve_multisig_request", json!({"request_id": 1})),
        (&m1, "execute_multisig_request", json!({"request_id": 1})),
        (
            &m1,
            "create_multisig_request",
            json!({"action": {"ConfigureMinterAllowance": {"controller_id": m1.id(), "minter_allowance": (1_000_000 * USDC).to_string()}}}),
        ),
        (&m1, "approve_multisig_request", json!({"request_id": 2})),
        (&m2, "approve_multisig_request", json!({"request_id": 2})),
        (&m1, "execute_multisig_request", json!({"request_id": 2})),
    ];
    for (quien, metodo, args) in pasos {
        let r = quien
            .call(usdc.id(), metodo)
            .args_json(args)
            .max_gas()
            .transact()
            .await?;
        assert!(r.is_success(), "{metodo}: {:?}", r.into_result());
    }
    let owner = sub(&root, "owner", 10).await?;
    let guardian = sub(&root, "guardian", 5).await?;
    let capturer = sub(&root, "capturer", 10).await?;
    let fees = sub(&root, "fees", 5).await?;
    let payer = sub(&root, "payer", 10).await?;
    let worker = sub(&root, "worker", 5).await?;
    let arras = deploy_arras(
        &root,
        "arras",
        owner.id(),
        guardian.id(),
        usdc.id(),
        10 * USDC,
    )
    .await?;
    for who in [arras.id(), payer.id(), worker.id(), fees.id()] {
        ft_register(&root, usdc.id(), who).await?;
    }
    let r = m1
        .call(usdc.id(), "mint")
        .args_json(json!({ "to": payer.id(), "amount": (100 * USDC).to_string() }))
        .max_gas()
        .transact()
        .await?;
    assert!(r.is_success(), "mint: {:?}", r.into_result());
    let o = Opts::default();
    let r = owner
        .call(arras.id(), "register_operator")
        .args_json(json!({ "operator_id": "em", "input": operator_input(owner.id(), capturer.id(), fees.id(), &o) }))
        .deposit(ONE_YOCTO)
        .transact()
        .await?;
    assert!(r.is_success());
    nep145(&payer, arras.id(), NearToken::from_millinear(50)).await?;
    let now = w.view_block().await?.timestamp();
    let ft = usdc.as_account().clone();
    Ok(EnvUsdc {
        e: Env {
            w,
            root,
            ft,
            arras,
            owner,
            capturer,
            fees,
            payer,
            worker,
            now,
        },
    })
}

/// Las cadenas `ed25519:<b58>` del outcome (public key y firma del sandbox, datos publicos) se graban sin el
/// prefijo: near-crypto las sigue leyendo (ed25519 es el tipo por defecto) y el escaneo de secretos del repo no
/// da falsos positivos.
fn sin_prefijo_ed25519(v: &mut Value) {
    match v {
        Value::String(s) => {
            if let Some(rest) = s.strip_prefix("ed25519:") {
                *s = rest.to_string();
            }
        }
        Value::Array(a) => a.iter_mut().for_each(sin_prefijo_ed25519),
        Value::Object(o) => o.values_mut().for_each(sin_prefijo_ed25519),
        _ => {}
    }
}

/// Lo que H-4 necesita del fixture: el `ft_resolve_transfer` en `"0"`, el reembolso `nep141 ft_transfer` del
/// contrato al payer por el 100 % y el log del rechazo.
fn validar_fixture_w13(f: &Value) {
    let status = &f["status"]["SuccessValue"];
    assert_eq!(status, "IjAi", "status = SuccessValue base64(\"0\")");
    let outcomes = f["receipts_outcome"].as_array().expect("receipts_outcome");
    let logs: Vec<&str> = outcomes
        .iter()
        .flat_map(|o| {
            o["outcome"]["logs"]
                .as_array()
                .unwrap()
                .iter()
                .map(|l| l.as_str().unwrap())
        })
        .collect();
    assert!(
        logs.contains(&"arras:rechazo:E06_TOPE_POR_PAGO"),
        "{logs:?}"
    );
    let reembolso = logs
        .iter()
        .filter_map(|l| l.strip_prefix("EVENT_JSON:"))
        .any(|j| {
            let e: Value = serde_json::from_str(j).unwrap_or_default();
            e["standard"] == "nep141"
                && e["event"] == "ft_transfer"
                && e["data"][0]["old_owner_id"] == "arras.test.near"
                && e["data"][0]["new_owner_id"] == "payer.test.near"
                && e["data"][0]["amount"] == "10000001"
        });
    assert!(
        reembolso,
        "evento nep141 ft_transfer de reembolso: {logs:?}"
    );
    let resolve = f["receipts"]
        .as_array()
        .expect("receipts")
        .iter()
        .find(|r| {
            r["receipt"]["Action"]["actions"][0]["FunctionCall"]["method_name"]
                == "ft_resolve_transfer"
        })
        .expect("receipt ft_resolve_transfer");
    let rid = resolve["receipt_id"].as_str().unwrap();
    let out = outcomes
        .iter()
        .find(|o| o["id"] == rid)
        .expect("outcome de ft_resolve_transfer");
    assert_eq!(
        out["outcome"]["status"]["SuccessValue"], "IjAi",
        "ft_resolve_transfer devuelve \"0\""
    );
    assert_eq!(out["outcome"]["executor_id"], "usdc.test.near");
}

/// W-13 `rechazo_con_ft_real`: `E06` con el USDC real: `ft_resolve_transfer` devuelve `"0"`, el 100 % vuelve al
/// payer y sale el evento `nep141 ft_transfer` de reembolso. El outcome es el fixture de H-4.
#[tokio::test]
#[ignore = "necesita red (clona el USDC de testnet): ARRAS_TESTNET_RPC=https://rpc.testnet.fastnear.com cargo test -p arras-escrow-v1 --test workspaces -- --ignored"]
async fn w13_rechazo_con_ft_real() -> anyhow::Result<()> {
    let _permiso = SANDBOXES.acquire().await?;
    let EnvUsdc { e } = setup_usdc_real().await?;
    let monto = 10 * USDC + 1;
    let r = e
        .authorize(&e.payer, monto, &e.msg(e.worker.id(), 1))
        .await?;
    assert!(r.is_success(), "{:?}", r.clone().into_result());
    let used: String = r.clone().json()?;
    assert_eq!(used, "0", "ft_resolve_transfer devuelve \"0\"");
    assert!(
        r.logs().contains(&"arras:rechazo:E06_TOPE_POR_PAGO"),
        "{:?}",
        r.logs()
    );
    let reembolsos = events(&r, "nep141", "ft_transfer");
    assert_eq!(reembolsos.len(), 2, "ida y vuelta: {reembolsos:?}");
    let vuelta = &reembolsos[1]["data"][0];
    assert_eq!(
        (
            vuelta["old_owner_id"].as_str(),
            vuelta["new_owner_id"].as_str()
        ),
        (Some(e.arras.id().as_str()), Some(e.payer.id().as_str()))
    );
    assert_eq!(vuelta["amount"], monto.to_string());
    assert_eq!(
        e.ft_balance(e.payer.id()).await?,
        100 * USDC,
        "100 % de vuelta"
    );
    assert_eq!(e.ft_balance(e.arras.id()).await?, 0);
    assert_eq!(e.config().await?["payments"], 0);

    let body = json!({
        "jsonrpc": "2.0", "id": "w13", "method": "EXPERIMENTAL_tx_status",
        "params": { "tx_hash": r.outcome().transaction_hash.to_string(), "sender_account_id": e.payer.id(), "wait_until": "FINAL" },
    });
    let resp: Value = reqwest::Client::new()
        .post(e.w.rpc_addr())
        .json(&body)
        .send()
        .await?
        .json()
        .await?;
    let mut fixture = resp["result"].clone();
    assert!(fixture.is_object(), "EXPERIMENTAL_tx_status: {resp}");
    validar_fixture_w13(&fixture);
    if std::env::var("ARRAS_GRABAR_FIXTURE").as_deref() == Ok("1") {
        sin_prefijo_ed25519(&mut fixture);
        std::fs::write(FIXTURE_W13, serde_json::to_string_pretty(&fixture)? + "\n")?;
        println!("MEDIDA W-13 fixture grabado en contracts/escrow-v1/{FIXTURE_W13}");
    }
    let grabado: Value = serde_json::from_str(&std::fs::read_to_string(FIXTURE_W13)?)?;
    validar_fixture_w13(&grabado);
    Ok(())
}

/// W-13b: el flujo feliz con el USDC real (authorize -> release -> `withdraw_for`): los 10 TGas de
/// `GAS_FT_TRANSFER` alcanzan para el `ft_transfer` de Circle.
#[tokio::test]
#[ignore = "necesita red (clona el USDC de testnet): ARRAS_TESTNET_RPC=https://rpc.testnet.fastnear.com cargo test -p arras-escrow-v1 --test workspaces -- --ignored"]
async fn w13b_flujo_con_ft_real() -> anyhow::Result<()> {
    let _permiso = SANDBOXES.acquire().await?;
    let EnvUsdc { e } = setup_usdc_real().await?;
    let msg = e.msg(e.worker.id(), 1);
    let r = e.authorize(&e.payer, USDC, &msg).await?;
    assert_eq!(
        r.clone().json::<String>()?,
        USDC.to_string(),
        "{:?}",
        r.logs()
    );
    let id = e.payment_id(e.payer.id(), USDC, &msg).await?;
    let detalle: Vec<String> = r
        .receipt_outcomes()
        .iter()
        .map(|o| {
            format!(
                "{}={:.3}",
                o.executor_id,
                o.gas_burnt.as_gas() as f64 / 1e12
            )
        })
        .collect();
    // Prepagado minimo con el que el authorize pasa con el USDC real (pagos nuevos en cada intento).
    let (mut lo, mut hi) = (10u64, 150u64);
    let mut salt = 10u8;
    while hi - lo > 1 {
        let mid = (lo + hi) / 2;
        let m = e.msg(e.worker.id(), salt);
        salt += 1;
        let r = e
            .authorize_gas(&e.payer, 10_000, &m, Gas::from_tgas(mid))
            .await?;
        let ok = r.is_success()
            && r.clone()
                .json::<String>()
                .map(|u| u == "10000")
                .unwrap_or(false);
        if ok { hi = mid } else { lo = mid }
    }
    println!(
        "MEDIDA W-13b ft_transfer_call authorize con el USDC real: gas quemado total {:.3} TGas; por receipt [{}]; prepagado minimo que pasa {hi} TGas",
        r.total_gas_burnt.as_gas() as f64 / 1e12,
        detalle.join(", ")
    );
    e.release(&id, None).await?;
    let r = e
        .withdraw_for(&e.capturer, e.worker.id(), Gas::from_tgas(50))
        .await?;
    assert!(r.is_success(), "{:?}", r.clone().into_result());
    assert_eq!(withdrawn_ok(&r), vec![true]);
    assert_eq!(e.ft_balance(e.worker.id()).await?, 870_000);
    let ft_transfer = r
        .receipt_outcomes()
        .iter()
        .find(|o| &o.executor_id == e.ft.id() && o.logs.iter().any(|l| l.contains("ft_transfer")))
        .map(|o| o.gas_burnt.as_gas() as f64 / 1e12);
    println!(
        "MEDIDA W-13b ft_transfer de Circle quemo {:?} TGas (GAS_FT_TRANSFER = 10)",
        ft_transfer
    );
    e.invariante(&e.todas()).await?;
    Ok(())
}

/// W-09b: gas de cada metodo del ciclo de vida que relaya H-4, con el USDC real: quemado por la transaccion y
/// prepagado minimo que pasa (busqueda binaria). H-4 calibra `NEAR_ESCROW_*_GAS_TGAS` con esto mas un 20 %.
#[tokio::test]
#[ignore = "necesita red (clona el USDC de testnet): ARRAS_TESTNET_RPC=https://rpc.testnet.fastnear.com cargo test -p arras-escrow-v1 --test workspaces -- --ignored"]
async fn w09b_gas_por_metodo() -> anyhow::Result<()> {
    let _permiso = SANDBOXES.acquire().await?;
    let EnvUsdc { e } = setup_usdc_real().await?;
    let tg = |r: &ExecutionFinalResult| r.total_gas_burnt.as_gas() as f64 / 1e12;
    let mut filas: Vec<String> = vec![];
    // 0,05 NEAR de NEP-145 alcanzan para ~9 pagos (E14 despues): se recarga para los 17 de este test.
    nep145(&e.payer, e.arras.id(), NearToken::from_millinear(500)).await?;
    // Pagos con plazos cortos para poder llegar a reclaim con fast_forward.
    let corto = |salt: u8| {
        msg_for(
            e.worker.id(),
            e.now + 400 * 1_000_000_000,
            e.now + 401 * 1_000_000_000,
            salt,
        )
    };
    let mut largos = vec![];
    for salt in 0..8u8 {
        largos.push(
            e.authorize_ok(&e.payer, USDC, &e.msg(e.worker.id(), salt))
                .await?,
        );
    }
    let mut cortos = vec![];
    for salt in 100..108u8 {
        cortos.push(e.authorize_ok(&e.payer, USDC, &corto(salt)).await?);
    }

    // release (parcial de 1 sobre el mismo pago: no lo consume).
    let call_release = |gas: u64| {
        e.capturer
            .call(e.arras.id(), "release")
            .args_json(json!({ "payment_id": largos[0], "amount": "1" }))
            .gas(Gas::from_tgas(gas))
            .transact()
    };
    let r = call_release(300).await?;
    assert!(r.is_success(), "{:?}", r.into_result());
    let quemado = tg(&r);
    let (mut lo, mut hi) = (1u64, 40u64);
    while hi - lo > 1 {
        let mid = (lo + hi) / 2;
        if call_release(mid).await?.is_success() {
            hi = mid
        } else {
            lo = mid
        }
    }
    filas.push(format!(
        "release {quemado:.3} TGas quemado, minimo {hi} TGas"
    ));

    // refund (consume un pago por exito).
    let mut libres = largos[1..].to_vec();
    let mut quemado = 0.0;
    let (mut lo, mut hi) = (1u64, 40u64);
    while hi - lo > 1 {
        let mid = (lo + hi) / 2;
        let id = libres.last().unwrap().clone();
        let r = e
            .capturer
            .call(e.arras.id(), "refund")
            .args_json(json!({ "payment_id": id }))
            .gas(Gas::from_tgas(mid))
            .transact()
            .await?;
        if r.is_success() {
            quemado = tg(&r);
            libres.pop();
            hi = mid;
        } else {
            lo = mid;
        }
    }
    filas.push(format!(
        "refund {quemado:.3} TGas quemado, minimo {hi} TGas"
    ));

    // reclaim: se adelanta el reloj del sandbox mas alla de reclaim_after.
    let objetivo = e.now + 401 * 1_000_000_000;
    let antes = e.w.view_block().await?.timestamp();
    let mut bloques = 0u64;
    while e.w.view_block().await?.timestamp() <= objetivo {
        assert!(bloques < 200_000, "fast_forward no adelanta el reloj");
        e.w.fast_forward(5_000).await?;
        bloques += 5_000;
    }
    let ahora = e.w.view_block().await?.timestamp();
    println!(
        "MEDIDA W-09b fast_forward: {bloques} bloques adelantaron {:.1} s",
        (ahora - antes) as f64 / 1e9
    );
    let mut quemado = 0.0;
    let (mut lo, mut hi) = (1u64, 40u64);
    while hi - lo > 1 {
        let mid = (lo + hi) / 2;
        let id = cortos.last().unwrap().clone();
        let r = e
            .payer
            .call(e.arras.id(), "reclaim")
            .args_json(json!({ "payment_id": id }))
            .gas(Gas::from_tgas(mid))
            .transact()
            .await?;
        if r.is_success() {
            quemado = tg(&r);
            cortos.pop();
            hi = mid;
        } else {
            lo = mid;
        }
    }
    filas.push(format!(
        "reclaim {quemado:.3} TGas quemado, minimo {hi} TGas"
    ));

    // withdraw (payer, con lo reembolsado) y withdraw_for Ok (worker) y Err (cuenta sin registro en el USDC).
    let r = e
        .payer
        .call(e.arras.id(), "withdraw")
        .gas(Gas::from_tgas(50))
        .transact()
        .await?;
    assert_eq!(withdrawn_ok(&r), vec![true]);
    filas.push(format!("withdraw (payer, Ok) {:.3} TGas quemado", tg(&r)));
    let r = e
        .withdraw_for(&e.capturer, e.worker.id(), Gas::from_tgas(50))
        .await?;
    assert_eq!(withdrawn_ok(&r), vec![true]);
    filas.push(format!("withdraw_for (Ok) {:.3} TGas quemado", tg(&r)));
    // Una cuenta sin registro en el USDC con claimable: receiver de un pago liberado.
    let sin_usdc = sub(&e.root, "sinusdc", 5).await?;
    let m = msg_for(
        sin_usdc.id(),
        e.w.view_block().await?.timestamp() + DAY_NS,
        e.w.view_block().await?.timestamp() + 2 * DAY_NS,
        1,
    );
    let id = e.authorize_ok(&e.payer, USDC, &m).await?;
    e.release(&id, None).await?;
    let mut quemado_err = 0.0;
    let (mut lo, mut hi) = (1u64, 40u64);
    while hi - lo > 1 {
        let mid = (lo + hi) / 2;
        let r = e
            .withdraw_for(&e.capturer, sin_usdc.id(), Gas::from_tgas(mid))
            .await?;
        if withdrawn_ok(&r) == vec![false] {
            quemado_err = tg(&r);
            hi = mid;
        } else {
            assert!(
                withdrawn_ok(&r).is_empty(),
                "nunca cobra una cuenta sin registro"
            );
            lo = mid;
        }
    }
    assert_eq!(
        e.claimable(sin_usdc.id()).await?,
        870_000,
        "restaurado en todos los intentos"
    );
    filas.push(format!(
        "withdraw_for (Err, cuenta sin registro) {quemado_err:.3} TGas quemado, minimo {hi} TGas"
    ));
    println!(
        "MEDIDA W-09b gas por metodo con el USDC real: {}",
        filas.join("; ")
    );
    e.invariante(&[e.payer.id(), e.worker.id(), e.fees.id(), sin_usdc.id()])
        .await?;
    Ok(())
}

/// Sin red: el fixture W-13 commiteado (el `EXPERIMENTAL_tx_status` real del rechazo `E06` con el USDC de Circle)
/// tiene todo lo que H-4 lee. Los tests con red son `#[ignore]` (ver el encabezado).
#[test]
fn w13_fixture_grabado() -> anyhow::Result<()> {
    let grabado: Value = serde_json::from_str(&std::fs::read_to_string(FIXTURE_W13)?)?;
    validar_fixture_w13(&grabado);
    assert_eq!(grabado["final_execution_status"], "FINAL");
    assert_eq!(grabado["transaction"]["receiver_id"], "usdc.test.near");
    Ok(())
}
