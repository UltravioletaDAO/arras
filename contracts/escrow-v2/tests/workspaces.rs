//! Tests de v2 contra la wasm real en el sandbox de near-workspaces (FT de prueba de near-contract-standards).
//!
//! - W2-01: el presupuesto con una **function-call key de verdad** (la FullAccess del agente borrada): autoriza, y la
//!   llave no sirve para otra cosa (otro metodo, otro contrato, deposito).
//! - W2-02: topes del agente (por pago, diario y global) contra la wasm.
//! - W2-03 y W2-04: disputa resuelta por el arbitro y disputa vencida (el sandbox avanza el reloj con
//!   `fast_forward`).
//! - W2-05: `operator_ticket` contra el griefing del pool de storage (F-5).
//! - W2-06: gas minimo del callback de `withdraw_for` en v2 (como W-04 de v1).
//! - W2-07: griefing del TVL con depositos de presupuesto.
//! - W2-08: sin llaves despues del deploy.
//! - W2-09: gas por metodo (lineas `MEDIDA`, para los topes del facilitador).
//! - W2-10: invariante completa contra los saldos del token: lo que entra es lo que sale mas lo retenido.
//!
//! `cargo test -p arras-escrow-v2 --test workspaces -- --nocapture` imprime las `MEDIDA`.

use ed25519_dalek::{Signer, SigningKey};
use near_workspaces::network::Sandbox;
use near_workspaces::operations::Function;
use near_workspaces::result::ExecutionFinalResult;
use near_workspaces::types::{AccessKey, Gas, KeyType, NearToken, SecretKey};
use near_workspaces::{Account, AccountId, Worker};
use serde_json::{Value, json};
use tokio::sync::{OnceCell, Semaphore};

static WASM_V2: OnceCell<Vec<u8>> = OnceCell::const_new();
static WASM_FT: OnceCell<Vec<u8>> = OnceCell::const_new();
/// Cuantos sandboxes a la vez (cada test levanta el suyo).
static SANDBOXES: Semaphore = Semaphore::const_new(4);

const USDC: u128 = 1_000_000;
const ONE_YOCTO: NearToken = NearToken::from_yoctonear(1);
const DAY_NS: u64 = 86_400 * 1_000_000_000;
const MIN_WINDOW_NS: u64 = 60 * 1_000_000_000;
const RAZON: &str = "2222222222222222222222222222222222222222222222222222222222222222";

async fn wasm_v2() -> anyhow::Result<&'static [u8]> {
    let w = WASM_V2
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

/// Llave de tickets de los tests: semilla `02` x 32 (la de los vectores, `01` x 32, el contrato la rechaza).
fn ticket_key() -> SigningKey {
    SigningKey::from_bytes(&[2; 32])
}

fn ticket_pubkey_str() -> String {
    format!(
        "ed25519:{}",
        near_sdk::bs58::encode(ticket_key().verifying_key().to_bytes()).into_string()
    )
}

#[derive(Clone)]
struct Opts {
    fees_en_ft: bool,
    con_ticket: bool,
    arbiter_window_ns: u64,
    tope_tvl_por_payer: u128,
}

impl Default for Opts {
    fn default() -> Self {
        Self {
            fees_en_ft: true,
            con_ticket: false,
            arbiter_window_ns: 3 * DAY_NS,
            tope_tvl_por_payer: 20 * USDC,
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
    arbiter: Account,
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

/// Deploy + `new` en la misma transaccion (como `with-init-call`).
async fn deploy_arras(
    root: &Account,
    name: &str,
    owner: &AccountId,
    guardian: &AccountId,
    token: &AccountId,
) -> anyhow::Result<Account> {
    let acc = sub(root, name, 10).await?;
    let r = acc
        .batch(acc.id())
        .deploy(wasm_v2().await?)
        .call(
            Function::new("new")
                .args_json(json!({
                    "owner": owner,
                    "guardian": guardian,
                    "token": token,
                    "tope_por_pago": (10 * USDC).to_string(),
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

/// Sandbox con el FT de prueba, Arras v2 y el operador `em` con arbitro (y llave de tickets si se pide).
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
    let arbiter = sub(&root, "arbiter", 5).await?;
    let fees = sub(&root, "fees", 5).await?;
    let payer = sub(&root, "payer", 10).await?;
    let worker = sub(&root, "worker", 5).await?;
    let arras = deploy_arras(&root, "arras", owner.id(), guardian.id(), ft.id()).await?;
    for (who, si) in [
        (arras.id(), true),
        (payer.id(), true),
        (worker.id(), true),
        (fees.id(), o.fees_en_ft),
    ] {
        if si {
            ft_register(&root, ft.id(), who).await?;
        }
    }
    ft_send(&ft, ft.id(), payer.id(), 100 * USDC).await?;
    let mut input = json!({
        "owner": owner.id(),
        "capturers": [capturer.id()],
        "fee_bps": 1300,
        "fee_recipient": fees.id(),
        "min_amount": "10000",
        "tope_tvl_operador": (200 * USDC).to_string(),
        "tope_tvl_por_payer": o.tope_tvl_por_payer.to_string(),
        "sponsor_max_por_payer": NearToken::from_millinear(20).as_yoctonear().to_string(),
        "arbiter": arbiter.id(),
        "arbiter_window_ns": o.arbiter_window_ns.to_string(),
    });
    if o.con_ticket {
        input["ticket_pubkey"] = json!(ticket_pubkey_str());
    }
    let r = owner
        .call(arras.id(), "register_operator")
        .args_json(json!({ "operator_id": "em", "input": input }))
        .deposit(ONE_YOCTO)
        .transact()
        .await?;
    assert!(r.is_success(), "register_operator: {:?}", r.into_result());
    nep145(&payer, arras.id(), NearToken::from_millinear(50)).await?;
    let now = w.view_block().await?.timestamp();
    Ok(Env {
        w,
        root,
        ft,
        arras,
        owner,
        capturer,
        arbiter,
        fees,
        payer,
        worker,
        now,
    })
}

fn salt_hex(n: u8) -> String {
    format!("{}{n:02x}", "0".repeat(30))
}

impl Env {
    /// `msg` `authorize` canonico de v2: plazos de 1 y 2 dias desde el inicio del test.
    fn msg(
        &self,
        receiver: &AccountId,
        salt: u8,
        dispute_deadline: Option<u64>,
        ticket: Option<&Value>,
    ) -> String {
        let mut m = json!({
            "v": 1,
            "action": "authorize",
            "operator_id": "em",
            "receiver": receiver,
            "release_deadline": (self.now + DAY_NS).to_string(),
            "reclaim_after": (self.now + 2 * DAY_NS).to_string(),
            "fee_bps_max": 1300,
            "terms_hash": "0".repeat(64),
            "salt": salt_hex(salt),
        });
        if let Some(d) = dispute_deadline {
            m["dispute_deadline"] = json!(d.to_string());
        }
        if let Some(t) = ticket {
            m["operator_ticket"] = t.clone();
        }
        m.to_string()
    }

    fn budget_msg(
        &self,
        agent: &AccountId,
        tope_por_pago: u128,
        tope_diario: u128,
        ticket: Option<&Value>,
    ) -> String {
        let mut m = json!({
            "v": 1,
            "action": "budget",
            "operator_id": "em",
            "agent": agent,
            "tope_por_pago": tope_por_pago.to_string(),
            "tope_diario": tope_diario.to_string(),
        });
        if let Some(t) = ticket {
            m["operator_ticket"] = t.clone();
        }
        m.to_string()
    }

    /// Los argumentos de `authorize_from_budget` de un pago a WORKER.
    fn afb_args(&self, owner: &AccountId, amount: u128, salt: u8) -> Value {
        json!({
            "owner": owner,
            "operator_id": "em",
            "receiver": self.worker.id(),
            "amount": amount.to_string(),
            "release_deadline": (self.now + DAY_NS).to_string(),
            "reclaim_after": (self.now + 2 * DAY_NS).to_string(),
            "fee_bps_max": 1300,
            "terms_hash": "0".repeat(64),
            "salt": salt_hex(salt),
        })
    }

    async fn ft_call(
        &self,
        from: &Account,
        amount: u128,
        msg: &str,
    ) -> anyhow::Result<ExecutionFinalResult> {
        Ok(from
            .call(self.ft.id(), "ft_transfer_call")
            .args_json(
                json!({ "receiver_id": self.arras.id(), "amount": amount.to_string(), "msg": msg }),
            )
            .deposit(ONE_YOCTO)
            .gas(Gas::from_tgas(100))
            .transact()
            .await?)
    }

    /// `ft_transfer_call` que tiene que pasar: el token usa todo el monto.
    async fn ft_call_ok(&self, from: &Account, amount: u128, msg: &str) -> anyhow::Result<()> {
        let r = self.ft_call(from, amount, msg).await?;
        let used: String = r.clone().json()?;
        assert_eq!(used, amount.to_string(), "rechazado: {:?}", r.logs());
        Ok(())
    }

    /// `ft_transfer_call` que el contrato tiene que rechazar con `code`: el token devuelve todo.
    async fn ft_call_rechazo(
        &self,
        from: &Account,
        amount: u128,
        msg: &str,
        code: &str,
    ) -> anyhow::Result<()> {
        let antes = self.ft_balance(from.id()).await?;
        let r = self.ft_call(from, amount, msg).await?;
        let used: String = r.clone().json()?;
        assert_eq!(used, "0", "{:?}", r.logs());
        assert!(
            r.logs().contains(&format!("arras:rechazo:{code}").as_str()),
            "{:?}",
            r.logs()
        );
        assert_eq!(
            self.ft_balance(from.id()).await?,
            antes,
            "el rechazo devuelve todo"
        );
        Ok(())
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

    async fn payment_id(
        &self,
        payer: &AccountId,
        amount: u128,
        msg: &str,
    ) -> anyhow::Result<String> {
        self.view(
            "compute_payment_id",
            json!({ "sender_id": payer, "amount": amount.to_string(), "msg": msg }),
        )
        .await
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

    async fn payment(&self, id: &str) -> anyhow::Result<Value> {
        self.view("get_payment", json!({ "payment_id": id })).await
    }

    async fn call(
        &self,
        by: &Account,
        method: &str,
        args: Value,
    ) -> anyhow::Result<ExecutionFinalResult> {
        Ok(by
            .call(self.arras.id(), method)
            .args_json(args)
            .gas(Gas::from_tgas(50))
            .transact()
            .await?)
    }

    async fn call_ok_r(
        &self,
        by: &Account,
        method: &str,
        args: Value,
    ) -> anyhow::Result<ExecutionFinalResult> {
        let r = self.call(by, method, args).await?;
        assert!(r.is_success(), "{method}: {:?}", r.clone().into_result());
        Ok(r)
    }

    async fn call_ok(&self, by: &Account, method: &str, args: Value) -> anyhow::Result<()> {
        self.call_ok_r(by, method, args).await.map(|_| ())
    }

    async fn call_falla(
        &self,
        by: &Account,
        method: &str,
        args: Value,
        needle: &str,
    ) -> anyhow::Result<()> {
        let r = self.call(by, method, args).await?;
        let err = format!("{:?}", r.into_result().unwrap_err());
        assert!(err.contains(needle), "{method}: {err}");
        Ok(())
    }

    async fn withdraw_for(
        &self,
        who: &AccountId,
        gas: Gas,
    ) -> anyhow::Result<ExecutionFinalResult> {
        Ok(self
            .capturer
            .call(self.arras.id(), "withdraw_for")
            .args_json(json!({ "account": who }))
            .gas(gas)
            .transact()
            .await?)
    }

    /// Un agente con **solo** una function-call key para `authorize_from_budget` en Arras (la FullAccess de creacion
    /// se borra). Devuelve la cuenta firmando con esa llave.
    async fn agente_con_fc_key(&self, name: &str) -> anyhow::Result<Account> {
        let agent = sub(&self.root, name, 2).await?;
        let fc = SecretKey::from_random(KeyType::ED25519);
        let r = agent
            .batch(agent.id())
            .add_key(
                fc.public_key(),
                AccessKey::function_call_access(
                    self.arras.id(),
                    &["authorize_from_budget"],
                    Some(NearToken::from_millinear(250)),
                ),
            )
            .delete_key(agent.secret_key().public_key())
            .transact()
            .await?;
        assert!(
            r.is_success(),
            "add_key + delete_key: {:?}",
            r.into_result()
        );
        let keys = self.w.view_access_keys(agent.id()).await?;
        assert_eq!(keys.len(), 1, "solo la function-call key");
        assert!(
            format!("{:?}", keys[0].access_key.permission).contains("FunctionCall"),
            "{:?}",
            keys[0].access_key.permission
        );
        Ok(Account::from_secret_key(agent.id().clone(), fc, &self.w))
    }

    /// Ticket firmado con la llave de los tests sobre los bytes que da la vista del contrato (y que coinciden con
    /// los del crate).
    async fn ticket_pago(&self, payment_id: &str, expiry: u64) -> anyhow::Result<Value> {
        let b64: near_sdk::json_types::Base64VecU8 = self
            .view(
                "ticket_message_payment",
                json!({ "payment_id": payment_id, "expiry": expiry.to_string() }),
            )
            .await?;
        let local = arras_escrow_v2::ticket::ticket_message(
            self.arras.id().as_str(),
            arras_escrow_v2::ticket::TicketSubject::Payment { payment_id },
            expiry,
        );
        assert_eq!(b64.0, local, "la vista y el crate firman lo mismo");
        let sig = ticket_key().sign(&b64.0).to_bytes();
        Ok(
            json!({ "expiry": expiry.to_string(), "sig": near_sdk::bs58::encode(sig).into_string() }),
        )
    }

    async fn ticket_budget(
        &self,
        owner: &AccountId,
        agent: &AccountId,
        expiry: u64,
    ) -> anyhow::Result<Value> {
        let b64: near_sdk::json_types::Base64VecU8 = self
            .view(
                "ticket_message_budget",
                json!({ "operator_id": "em", "owner": owner, "agent": agent, "expiry": expiry.to_string() }),
            )
            .await?;
        let sig = ticket_key().sign(&b64.0).to_bytes();
        Ok(
            json!({ "expiry": expiry.to_string(), "sig": near_sdk::bs58::encode(sig).into_string() }),
        )
    }

    /// La invariante completa contra la cadena: `sum(en escrow) + sum(claimable) + in_flight + budgets_total ==
    /// tvl_global == ft_balance_of(contrato)` (sin donaciones ni retiros en vuelo: lo que entra es lo que sale mas
    /// lo retenido).
    async fn invariante(&self, cuentas: &[&AccountId]) -> anyhow::Result<()> {
        let cfg = self.config().await?;
        let n = |v: &Value| v.as_str().unwrap().parse::<u128>().unwrap();
        let tvl = n(&cfg["tvl_global"]);
        let pagos: Vec<(String, Value)> = self
            .view(
                "list_payments",
                json!({ "operator_id": "em", "from_index": 0, "limit": 50 }),
            )
            .await?;
        let mut en_escrow = 0u128;
        for (_, p) in &pagos {
            en_escrow += n(&p["amount"]) - n(&p["released"]) - n(&p["refunded"]);
        }
        let mut claim = 0u128;
        for c in cuentas {
            claim += self.claimable(c).await?;
        }
        assert_eq!(
            en_escrow + claim + n(&cfg["in_flight"]) + n(&cfg["budgets_total"]),
            tvl,
            "sum(en escrow) + sum(claimable) + in_flight + budgets_total == tvl_global"
        );
        assert_eq!(n(&cfg["in_flight"]), 0);
        assert_eq!(
            tvl,
            self.ft_balance(self.arras.id()).await?,
            "tvl_global == ft_balance_of(contrato)"
        );
        Ok(())
    }
}

fn events(r: &ExecutionFinalResult, event: &str) -> Vec<Value> {
    r.logs()
        .iter()
        .filter_map(|l| l.strip_prefix("EVENT_JSON:"))
        .filter_map(|j| serde_json::from_str::<Value>(j).ok())
        .filter(|e| e["standard"] == "arras" && e["event"] == event)
        .collect()
}

fn withdrawn_ok(r: &ExecutionFinalResult) -> Vec<bool> {
    events(r, "withdrawn")
        .iter()
        .map(|e| e["data"][0]["ok"].as_bool().unwrap())
        .collect()
}

/// `new` es `#[private]` tambien en v2, y la version es `arras-v2`.
#[tokio::test]
async fn u24_new_ajeno() -> anyhow::Result<()> {
    let _permiso = SANDBOXES.acquire().await?;
    let worker = near_workspaces::sandbox().await?;
    let contract = worker.dev_deploy(wasm_v2().await?).await?;
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
    let err = format!("{:?}", r.into_result().unwrap_err());
    assert!(err.contains("Method new is private"), "{err}");
    let ok = contract.call("new").args_json(&args).transact().await?;
    assert!(ok.is_success(), "{:?}", ok.into_result());
    let cfg: Value = contract.view("get_config").await?.json()?;
    assert_eq!(cfg["version"], "arras-v2");
    assert_eq!(cfg["budgets_total"], "0");
    Ok(())
}

/// W2-01: el humano deposita; el agente, con **solo** una function-call key limitada a `authorize_from_budget` en
/// Arras, autoriza; el capturer libera y el worker cobra. La llave no sirve para otro metodo, otro contrato ni
/// para adjuntar deposito.
#[tokio::test]
async fn w2_01_presupuesto_con_function_call_key() -> anyhow::Result<()> {
    let _permiso = SANDBOXES.acquire().await?;
    let e = setup(Opts::default()).await?;
    let agent = e.agente_con_fc_key("agent").await?;
    e.ft_call_ok(
        &e.payer,
        5 * USDC,
        &e.budget_msg(agent.id(), 2 * USDC, 3 * USDC, None),
    )
    .await?;
    let budget: Value = e
        .view(
            "get_budget",
            json!({ "owner": e.payer.id(), "agent": agent.id(), "operator_id": "em" }),
        )
        .await?;
    assert_eq!(budget["saldo"], "5000000");
    e.invariante(&[e.payer.id(), e.worker.id(), e.fees.id()])
        .await?;

    let args = e.afb_args(e.payer.id(), USDC, 1);
    let r = agent
        .call(e.arras.id(), "authorize_from_budget")
        .args_json(&args)
        .gas(Gas::from_tgas(30))
        .transact()
        .await?;
    assert!(
        r.is_success(),
        "authorize_from_budget con la function-call key: {:?}",
        r.clone().into_result()
    );
    let id: String = r.clone().json()?;
    let mut sin_ticket = args.clone();
    sin_ticket.as_object_mut().unwrap().remove("owner");
    let esperado: String = e
        .view(
            "compute_budget_payment_id",
            json!({
                "owner": e.payer.id(), "operator_id": "em", "receiver": e.worker.id(), "amount": USDC.to_string(),
                "release_deadline": args["release_deadline"], "reclaim_after": args["reclaim_after"],
                "fee_bps_max": 1300, "terms_hash": "0".repeat(64), "salt": salt_hex(1),
            }),
        )
        .await?;
    assert_eq!(id, esperado);
    assert_eq!(events(&r, "authorized").len(), 1);
    assert_eq!(
        events(&r, "budget_spent")[0]["data"][0]["agent"],
        agent.id().as_str()
    );
    let p = e.payment(&id).await?;
    assert_eq!(
        p["payer"],
        e.payer.id().as_str(),
        "el payer del pago es el humano"
    );

    // La llave no sirve para otra cosa: otro metodo del contrato, otro contrato (el token) ni deposito.
    let r = agent
        .call(e.arras.id(), "withdraw_budget")
        .args_json(json!({ "agent": agent.id(), "operator_id": "em" }))
        .transact()
        .await;
    assert!(
        r.is_err() || r.unwrap().is_failure(),
        "otro metodo: rechazado por la llave"
    );
    let r = agent
        .call(e.ft.id(), "ft_transfer")
        .args_json(json!({ "receiver_id": agent.id(), "amount": "1" }))
        .deposit(ONE_YOCTO)
        .transact()
        .await;
    assert!(
        r.is_err() || r.unwrap().is_failure(),
        "otro contrato: rechazado por la llave"
    );
    let r = agent
        .call(e.arras.id(), "authorize_from_budget")
        .args_json(e.afb_args(e.payer.id(), USDC, 2))
        .deposit(ONE_YOCTO)
        .transact()
        .await;
    assert!(r.is_err() || r.unwrap().is_failure(), "deposito: rechazado");

    // El capturer libera y retira por el worker y el fee_recipient.
    e.call_ok(&e.capturer, "release", json!({ "payment_id": id }))
        .await?;
    for who in [e.worker.id(), e.fees.id()] {
        let r = e.withdraw_for(who, Gas::from_tgas(50)).await?;
        assert_eq!(withdrawn_ok(&r), vec![true]);
    }
    assert_eq!(e.ft_balance(e.worker.id()).await?, 870_000);
    assert_eq!(e.ft_balance(e.fees.id()).await?, 130_000);
    // El humano recupera el resto del presupuesto.
    e.call_ok(
        &e.payer,
        "withdraw_budget",
        json!({ "agent": agent.id(), "operator_id": "em" }),
    )
    .await?;
    let r = e.call_ok_r(&e.payer, "withdraw", json!({})).await?;
    assert_eq!(withdrawn_ok(&r), vec![true]);
    assert_eq!(e.ft_balance(e.payer.id()).await?, 99 * USDC);
    assert_eq!(e.ft_balance(e.arras.id()).await?, 0);
    let cfg = e.config().await?;
    assert_eq!(
        (cfg["tvl_global"].as_str(), cfg["budgets_total"].as_str()),
        (Some("0"), Some("0"))
    );
    e.invariante(&[e.payer.id(), e.worker.id(), e.fees.id()])
        .await?;
    Ok(())
}

/// W2-02: los topes del agente en la wasm: por pago (E52), global (E06), diario (E51) y saldo (E50); el revocado no
/// autoriza mas.
#[tokio::test]
async fn w2_02_topes_del_agente() -> anyhow::Result<()> {
    let _permiso = SANDBOXES.acquire().await?;
    let e = setup(Opts::default()).await?;
    let agent = e.agente_con_fc_key("agent").await?;
    e.ft_call_ok(
        &e.payer,
        10 * USDC,
        &e.budget_msg(agent.id(), 2 * USDC, 3 * USDC, None),
    )
    .await?;
    let afb = |amount: u128, salt: u8| {
        let args = e.afb_args(e.payer.id(), amount, salt);
        let agent = agent.clone();
        let arras = e.arras.id().clone();
        async move {
            agent
                .call(&arras, "authorize_from_budget")
                .args_json(args)
                .gas(Gas::from_tgas(30))
                .transact()
                .await
        }
    };
    let falla = |r: ExecutionFinalResult, needle: &str| {
        let err = format!("{:?}", r.into_result().unwrap_err());
        assert!(err.contains(needle), "{err}");
    };
    falla(afb(2 * USDC + 1, 1).await?, "ARRAS_E52");
    falla(afb(10 * USDC + 1, 1).await?, "ARRAS_E06");
    // Horizonte: el agente no elige un reclaim_after a mas de 30 dias (refutacion de v2, P1-1).
    let mut lejos = e.afb_args(e.payer.id(), USDC, 9);
    lejos["release_deadline"] = json!((e.now + 40 * DAY_NS).to_string());
    lejos["reclaim_after"] = json!((e.now + 41 * DAY_NS).to_string());
    falla(
        agent
            .call(e.arras.id(), "authorize_from_budget")
            .args_json(lejos)
            .gas(Gas::from_tgas(30))
            .transact()
            .await?,
        "ARRAS_E57",
    );
    assert!(afb(2 * USDC, 1).await?.is_success());
    assert!(afb(USDC, 2).await?.is_success());
    falla(afb(10_000, 3).await?, "ARRAS_E51");
    let b: Value = e
        .view(
            "get_budget",
            json!({ "owner": e.payer.id(), "agent": agent.id(), "operator_id": "em" }),
        )
        .await?;
    assert_eq!(
        (b["saldo"].as_str(), b["disponible_hoy"].as_str()),
        (Some("7000000"), Some("0"))
    );
    e.call_ok(
        &e.payer,
        "revoke_agent",
        json!({ "agent": agent.id(), "operator_id": "em" }),
    )
    .await?;
    falla(afb(10_000, 4).await?, "ARRAS_E50");
    assert_eq!(e.claimable(e.payer.id()).await?, 7 * USDC);
    e.invariante(&[e.payer.id(), e.worker.id(), e.fees.id()])
        .await?;
    Ok(())
}

/// W2-03: el payer abre la disputa, el capturer ya no libera, el arbitro reparte 40/60 y todos cobran exacto.
#[tokio::test]
async fn w2_03_disputa_resuelta_por_el_arbitro() -> anyhow::Result<()> {
    let _permiso = SANDBOXES.acquire().await?;
    let e = setup(Opts::default()).await?;
    let msg = e.msg(e.worker.id(), 1, None, None);
    e.ft_call_ok(&e.payer, USDC, &msg).await?;
    let id = e.payment_id(e.payer.id(), USDC, &msg).await?;
    let r = e
        .call_ok_r(
            &e.payer,
            "open_dispute",
            json!({ "payment_id": id, "reason_hash": RAZON }),
        )
        .await?;
    assert_eq!(
        events(&r, "dispute_opened")[0]["data"][0]["amount"],
        "1000000"
    );
    e.call_falla(
        &e.capturer,
        "release",
        json!({ "payment_id": id }),
        "ARRAS_E22",
    )
    .await?;
    e.call_falla(
        &e.capturer,
        "resolve_dispute",
        json!({ "payment_id": id, "to_receiver": "400000", "to_payer": "600000" }),
        "ARRAS_E62",
    )
    .await?;
    e.call_falla(
        &e.arbiter,
        "resolve_dispute",
        json!({ "payment_id": id, "to_receiver": "400000", "to_payer": "600001" }),
        "ARRAS_E63",
    )
    .await?;
    let r = e
        .call_ok_r(
            &e.arbiter,
            "resolve_dispute",
            json!({ "payment_id": id, "to_receiver": "400000", "to_payer": "600000" }),
        )
        .await?;
    let d = &events(&r, "dispute_resolved")[0]["data"][0];
    assert_eq!(
        (d["net"].as_str(), d["fee"].as_str(), d["expired"].as_bool()),
        (Some("348000"), Some("52000"), Some(false))
    );
    assert_eq!(e.payment(&id).await?["status"], "Resolved");
    for who in [e.worker.id(), e.fees.id(), e.payer.id()] {
        let r = e.withdraw_for(who, Gas::from_tgas(50)).await?;
        assert_eq!(withdrawn_ok(&r), vec![true]);
    }
    assert_eq!(e.ft_balance(e.worker.id()).await?, 348_000);
    assert_eq!(e.ft_balance(e.fees.id()).await?, 52_000);
    assert_eq!(e.ft_balance(e.payer.id()).await?, 99 * USDC + 600_000);
    assert_eq!(e.ft_balance(e.arras.id()).await?, 0);
    e.invariante(&[e.payer.id(), e.worker.id(), e.fees.id()])
        .await?;
    Ok(())
}

/// W2-04: plazo del arbitro de un minuto; el payer abre y el arbitro no aparece: antes del plazo `E64`, despues
/// cualquiera la cierra y lo vivo va al receiver (la parte que no abrio).
#[tokio::test]
async fn w2_04_disputa_vencida() -> anyhow::Result<()> {
    let _permiso = SANDBOXES.acquire().await?;
    let e = setup(Opts {
        arbiter_window_ns: MIN_WINDOW_NS,
        ..Default::default()
    })
    .await?;
    let msg = e.msg(e.worker.id(), 1, None, None);
    e.ft_call_ok(&e.payer, USDC, &msg).await?;
    let id = e.payment_id(e.payer.id(), USDC, &msg).await?;
    let r = e
        .call_ok_r(
            &e.payer,
            "open_dispute",
            json!({ "payment_id": id, "reason_hash": RAZON }),
        )
        .await?;
    let resolve_by: u64 = events(&r, "dispute_opened")[0]["data"][0]["resolve_by"]
        .as_str()
        .unwrap()
        .parse()?;
    let tercero = sub(&e.root, "tercero", 2).await?;
    e.call_falla(
        &tercero,
        "resolve_expired_dispute",
        json!({ "payment_id": id }),
        "ARRAS_E64",
    )
    .await?;
    // El reloj del sandbox avanza con los bloques.
    let mut saltos = 0;
    while e.w.view_block().await?.timestamp() <= resolve_by {
        e.w.fast_forward(100).await?;
        saltos += 1;
        assert!(saltos < 100, "el reloj del sandbox no avanza");
    }
    e.call_falla(
        &e.arbiter,
        "resolve_dispute",
        json!({ "payment_id": id, "to_receiver": "0", "to_payer": "1000000" }),
        "ARRAS_E65",
    )
    .await?;
    let r = e
        .call_ok_r(
            &tercero,
            "resolve_expired_dispute",
            json!({ "payment_id": id }),
        )
        .await?;
    let d = &events(&r, "dispute_resolved")[0]["data"][0];
    assert_eq!(
        (
            d["to_receiver"].as_str(),
            d["to_payer"].as_str(),
            d["expired"].as_bool()
        ),
        (Some("1000000"), Some("0"), Some(true))
    );
    assert_eq!(e.claimable(e.worker.id()).await?, 870_000);
    assert_eq!(e.claimable(e.fees.id()).await?, 130_000);
    e.invariante(&[e.payer.id(), e.worker.id(), e.fees.id()])
        .await?;
    println!("MEDIDA W2-04 fast_forward(100) x {saltos} para pasar un plazo de 60 s");
    Ok(())
}

/// W2-05 (F-5): con `ticket_pubkey`, cuentas descartables sin NEP-145 no gastan el `sponsor_pool` ni ocupan TVL; un
/// payer con ticket si.
#[tokio::test]
async fn w2_05_ticket_contra_el_griefing_del_pool() -> anyhow::Result<()> {
    let _permiso = SANDBOXES.acquire().await?;
    let e = setup(Opts {
        con_ticket: true,
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
    let pool_antes: Value = e
        .view("get_operator", json!({ "operator_id": "em" }))
        .await?;
    for n in 0..5u8 {
        let atacante = sub(&e.root, &format!("atacante{n}"), 2).await?;
        ft_register(&e.root, e.ft.id(), atacante.id()).await?;
        ft_send(&e.ft, e.ft.id(), atacante.id(), USDC).await?;
        e.ft_call_rechazo(
            &atacante,
            10_000,
            &e.msg(e.worker.id(), n, None, None),
            "E53_TICKET_INVALIDO",
        )
        .await?;
        // Un ticket de otro pago tampoco sirve.
        let otro = e
            .payment_id(
                atacante.id(),
                10_000,
                &e.msg(e.worker.id(), n + 100, None, None),
            )
            .await?;
        let t = e.ticket_pago(&otro, e.now + DAY_NS).await?;
        e.ft_call_rechazo(
            &atacante,
            10_000,
            &e.msg(e.worker.id(), n, None, Some(&t)),
            "E53_TICKET_INVALIDO",
        )
        .await?;
    }
    let pool: Value = e
        .view("get_operator", json!({ "operator_id": "em" }))
        .await?;
    assert_eq!(
        pool["sponsor_pool"], pool_antes["sponsor_pool"],
        "el pool quedo intacto"
    );
    assert_eq!(e.config().await?["payments"], 0);
    // Con el ticket del pago: pasa (el msg sin ticket da el mismo id).
    let sin = e.msg(e.worker.id(), 1, None, None);
    let id = e.payment_id(e.payer.id(), USDC, &sin).await?;
    let t = e.ticket_pago(&id, e.now + DAY_NS).await?;
    let con = e.msg(e.worker.id(), 1, None, Some(&t));
    assert_eq!(e.payment_id(e.payer.id(), USDC, &con).await?, id);
    e.ft_call_ok(&e.payer, USDC, &con).await?;
    // Vencido: E54.
    let vencido = e
        .ticket_pago(
            &e.payment_id(e.payer.id(), USDC, &e.msg(e.worker.id(), 2, None, None))
                .await?,
            e.now,
        )
        .await?;
    e.ft_call_rechazo(
        &e.payer,
        USDC,
        &e.msg(e.worker.id(), 2, None, Some(&vencido)),
        "E54_TICKET_VENCIDO",
    )
    .await?;
    // Deposito de presupuesto: hace falta el ticket del par (owner, agent).
    let agent = e.agente_con_fc_key("agent").await?;
    e.ft_call_rechazo(
        &e.payer,
        USDC,
        &e.budget_msg(agent.id(), USDC, USDC, None),
        "E53_TICKET_INVALIDO",
    )
    .await?;
    let tb = e
        .ticket_budget(e.payer.id(), agent.id(), e.now + DAY_NS)
        .await?;
    e.ft_call_ok(
        &e.payer,
        USDC,
        &e.budget_msg(agent.id(), USDC, USDC, Some(&tb)),
    )
    .await?;
    // Y el pago del agente lleva el suyo.
    let mut args = e.afb_args(e.payer.id(), USDC, 3);
    let r = agent
        .call(e.arras.id(), "authorize_from_budget")
        .args_json(&args)
        .gas(Gas::from_tgas(30))
        .transact()
        .await?;
    assert!(format!("{:?}", r.into_result().unwrap_err()).contains("ARRAS_E53"));
    let aid: String = e
        .view(
            "compute_budget_payment_id",
            json!({
                "owner": e.payer.id(), "operator_id": "em", "receiver": e.worker.id(), "amount": USDC.to_string(),
                "release_deadline": args["release_deadline"], "reclaim_after": args["reclaim_after"],
                "fee_bps_max": 1300, "terms_hash": "0".repeat(64), "salt": salt_hex(3),
            }),
        )
        .await?;
    args["operator_ticket"] = e.ticket_pago(&aid, e.now + DAY_NS).await?;
    let r = agent
        .call(e.arras.id(), "authorize_from_budget")
        .args_json(&args)
        .gas(Gas::from_tgas(30))
        .transact()
        .await?;
    assert!(r.is_success(), "{:?}", r.into_result());
    e.invariante(&[e.payer.id(), e.worker.id(), e.fees.id()])
        .await?;
    Ok(())
}

/// W2-06: gas minimo del callback en v2 (como W-04): con cualquier gas, o la llamada entera falla (nada cambia) o el
/// callback corre y no se pierde nada, con la pata buena y con la que falla (fee_recipient sin registro).
#[tokio::test]
async fn w2_06_callback_gas_minimo() -> anyhow::Result<()> {
    let _permiso = SANDBOXES.acquire().await?;
    let e = setup(Opts {
        fees_en_ft: false,
        ..Default::default()
    })
    .await?;
    let msg = e.msg(e.worker.id(), 1, None, None);
    e.ft_call_ok(&e.payer, USDC, &msg).await?;
    let id = e.payment_id(e.payer.id(), USDC, &msg).await?;
    e.call_ok(&e.capturer, "release", json!({ "payment_id": id }))
        .await?;
    let mut minimos = vec![];
    for (who, esperado, registrado) in [
        (e.worker.id(), 870_000u128, true),
        (e.fees.id(), 130_000u128, false),
    ] {
        let mut minimo = None;
        for tgas in 10..=40u64 {
            let r = e.withdraw_for(who, Gas::from_tgas(tgas)).await?;
            assert_eq!(
                e.config().await?["in_flight"],
                "0",
                "nada queda en vuelo con {tgas} TGas"
            );
            let claim = e.claimable(who).await?;
            let recibido = e.ft_balance(who).await.unwrap_or(0);
            if events(&r, "withdrawn").is_empty() {
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
                assert_eq!((claim, recibido), (0, esperado));
            } else {
                assert_eq!(withdrawn_ok(&r), vec![false]);
                assert_eq!(claim, esperado, "restaurado con {tgas} TGas");
            }
            minimo = Some(tgas);
            break;
        }
        minimos.push(minimo.expect("algun gas alcanza"));
    }
    println!(
        "MEDIDA W2-06 gas minimo de withdraw_for en v2: {} TGas (ft_transfer ok), {} TGas (ft_transfer falla)",
        minimos[0], minimos[1]
    );
    Ok(())
}

/// W2-07: un payer llena su `tope_tvl_por_payer` con un presupuesto -> el siguiente deposito o pago es `E08`; otro
/// payer sigue.
#[tokio::test]
async fn w2_07_griefing_tvl_con_presupuesto() -> anyhow::Result<()> {
    let _permiso = SANDBOXES.acquire().await?;
    let e = setup(Opts::default()).await?;
    let agent = sub(&e.root, "agent", 2).await?;
    e.ft_call_ok(
        &e.payer,
        20 * USDC,
        &e.budget_msg(agent.id(), USDC, USDC, None),
    )
    .await?;
    e.ft_call_rechazo(
        &e.payer,
        10_000,
        &e.budget_msg(agent.id(), USDC, USDC, None),
        "E08_TVL_PAYER",
    )
    .await?;
    e.ft_call_rechazo(
        &e.payer,
        10_000,
        &e.msg(e.worker.id(), 1, None, None),
        "E08_TVL_PAYER",
    )
    .await?;
    let otro = sub(&e.root, "otro", 5).await?;
    ft_register(&e.root, e.ft.id(), otro.id()).await?;
    ft_send(&e.ft, e.ft.id(), otro.id(), 10 * USDC).await?;
    nep145(&otro, e.arras.id(), NearToken::from_millinear(50)).await?;
    e.ft_call_ok(&otro, 10 * USDC, &e.msg(e.worker.id(), 1, None, None))
        .await?;
    e.invariante(&[e.payer.id(), e.worker.id(), e.fees.id(), otro.id()])
        .await?;
    Ok(())
}

/// W2-08: tras deploy + borrar la llave, `view_access_key_list` vacio y el contrato sigue respondiendo.
#[tokio::test]
async fn w2_08_sin_llaves() -> anyhow::Result<()> {
    let _permiso = SANDBOXES.acquire().await?;
    let w = near_workspaces::sandbox().await?;
    let root = w.root_account()?;
    let owner = sub(&root, "owner", 5).await?;
    let arras = deploy_arras(&root, "v2", owner.id(), owner.id(), root.id()).await?;
    let keys = w.view_access_keys(arras.id()).await?;
    assert_eq!(keys.len(), 1);
    let r = arras
        .batch(arras.id())
        .delete_key(keys[0].public_key.clone())
        .transact()
        .await?;
    assert!(r.is_success(), "{:?}", r.into_result());
    assert!(w.view_access_keys(arras.id()).await?.is_empty());
    let r = arras
        .batch(arras.id())
        .deploy(wasm_v2().await?)
        .transact()
        .await;
    assert!(
        r.is_err() || r.unwrap().is_failure(),
        "sin llaves no hay redeploy"
    );
    let cfg: Value = root.view(arras.id(), "get_config").await?.json()?;
    assert_eq!(cfg["version"], "arras-v2");
    let r = owner
        .call(arras.id(), "pause")
        .deposit(ONE_YOCTO)
        .transact()
        .await?;
    assert!(r.is_success(), "{:?}", r.into_result());
    Ok(())
}

/// W2-09: gas quemado por metodo (para los topes por metodo del facilitador: 30 TGas para `authorize_from_budget`,
/// `open_dispute` y `resolve_*`).
#[tokio::test]
async fn w2_09_gas_por_metodo() -> anyhow::Result<()> {
    let _permiso = SANDBOXES.acquire().await?;
    let e = setup(Opts {
        con_ticket: true,
        ..Default::default()
    })
    .await?;
    let tg = |r: &ExecutionFinalResult| r.total_gas_burnt.as_gas() as f64 / 1e12;
    let mut medidas = vec![];
    let agent = e.agente_con_fc_key("agent").await?;
    let tb = e
        .ticket_budget(e.payer.id(), agent.id(), e.now + DAY_NS)
        .await?;
    let r = e
        .ft_call(
            &e.payer,
            5 * USDC,
            &e.budget_msg(agent.id(), 2 * USDC, 5 * USDC, Some(&tb)),
        )
        .await?;
    medidas.push((
        "ft_transfer_call budget (primer deposito, con ticket)",
        tg(&r),
    ));
    let r = e
        .ft_call(
            &e.payer,
            USDC,
            &e.budget_msg(agent.id(), 2 * USDC, 5 * USDC, Some(&tb)),
        )
        .await?;
    medidas.push(("ft_transfer_call budget (re-deposito)", tg(&r)));
    let sin = e.msg(e.worker.id(), 1, Some(e.now + DAY_NS), None);
    let id = e.payment_id(e.payer.id(), USDC, &sin).await?;
    let t = e.ticket_pago(&id, e.now + DAY_NS).await?;
    let r = e
        .ft_call(
            &e.payer,
            USDC,
            &e.msg(e.worker.id(), 1, Some(e.now + DAY_NS), Some(&t)),
        )
        .await?;
    medidas.push(("ft_transfer_call authorize (con ticket)", tg(&r)));
    let mut args = e.afb_args(e.payer.id(), USDC, 2);
    let aid: String = e
        .view(
            "compute_budget_payment_id",
            json!({
                "owner": e.payer.id(), "operator_id": "em", "receiver": e.worker.id(), "amount": USDC.to_string(),
                "release_deadline": args["release_deadline"], "reclaim_after": args["reclaim_after"],
                "fee_bps_max": 1300, "terms_hash": "0".repeat(64), "salt": salt_hex(2),
            }),
        )
        .await?;
    args["operator_ticket"] = e.ticket_pago(&aid, e.now + DAY_NS).await?;
    let r = agent
        .call(e.arras.id(), "authorize_from_budget")
        .args_json(&args)
        .gas(Gas::from_tgas(30))
        .transact()
        .await?;
    assert!(r.is_success(), "{:?}", r.clone().into_result());
    let afb = tg(&r);
    medidas.push(("authorize_from_budget (con ticket, function-call key)", afb));
    let r = e
        .call_ok_r(
            &e.payer,
            "open_dispute",
            json!({ "payment_id": id, "reason_hash": RAZON }),
        )
        .await?;
    let open = tg(&r);
    medidas.push(("open_dispute", open));
    let r = e
        .call_ok_r(
            &e.arbiter,
            "resolve_dispute",
            json!({ "payment_id": id, "to_receiver": "500000", "to_payer": "500000" }),
        )
        .await?;
    let resolve = tg(&r);
    medidas.push(("resolve_dispute", resolve));
    let r = e
        .call_ok_r(&e.capturer, "release", json!({ "payment_id": aid }))
        .await?;
    medidas.push(("release", tg(&r)));
    let r = e
        .call_ok_r(
            &e.payer,
            "withdraw_budget",
            json!({ "agent": agent.id(), "operator_id": "em" }),
        )
        .await?;
    medidas.push(("withdraw_budget", tg(&r)));
    let r = e
        .payer
        .call(e.arras.id(), "set_budget_caps")
        .args_json(json!({ "agent": agent.id(), "operator_id": "em", "tope_por_pago": "1", "tope_diario": "1" }))
        .deposit(ONE_YOCTO)
        .transact()
        .await?;
    assert!(r.is_success());
    medidas.push(("set_budget_caps", tg(&r)));
    let r = e
        .call_ok_r(
            &e.payer,
            "revoke_agent",
            json!({ "agent": agent.id(), "operator_id": "em" }),
        )
        .await?;
    medidas.push(("revoke_agent", tg(&r)));
    let r = e.withdraw_for(e.worker.id(), Gas::from_tgas(50)).await?;
    medidas.push(("withdraw_for (ok)", tg(&r)));
    for (m, g) in &medidas {
        println!("MEDIDA W2-09 {m}: {g:.3} TGas quemados");
    }
    for g in [afb, open, resolve] {
        assert!(g < 30.0, "cabe en el tope de 30 TGas del facilitador: {g}");
    }
    e.invariante(&[e.payer.id(), e.worker.id(), e.fees.id()])
        .await?;
    Ok(())
}

/// W2-10: invariante completa contra la cadena en un recorrido mixto: depositos, pagos del agente y del payer,
/// release parcial, refund, disputa, revocacion y retiros. Al final el contrato queda en 0 y cada cuenta tiene
/// exactamente lo que le toca: lo que entra es lo que sale mas lo retenido.
#[tokio::test]
async fn w2_10_invariante_completa() -> anyhow::Result<()> {
    let _permiso = SANDBOXES.acquire().await?;
    let e = setup(Opts::default()).await?;
    let cuentas = [e.payer.id(), e.worker.id(), e.fees.id()];
    let agent = e.agente_con_fc_key("agent").await?;
    e.ft_call_ok(
        &e.payer,
        6 * USDC,
        &e.budget_msg(agent.id(), 3 * USDC, 6 * USDC, None),
    )
    .await?;
    e.invariante(&cuentas).await?;
    let mut ids = vec![];
    for (salt, amount) in [(1u8, 2 * USDC), (2, USDC), (3, 3 * USDC)] {
        let r = agent
            .call(e.arras.id(), "authorize_from_budget")
            .args_json(e.afb_args(e.payer.id(), amount, salt))
            .gas(Gas::from_tgas(30))
            .transact()
            .await?;
        assert!(r.is_success(), "{:?}", r.clone().into_result());
        ids.push(r.json::<String>()?);
        e.invariante(&cuentas).await?;
    }
    let msg = e.msg(e.worker.id(), 9, None, None);
    e.ft_call_ok(&e.payer, 4 * USDC, &msg).await?;
    let directo = e.payment_id(e.payer.id(), 4 * USDC, &msg).await?;
    e.invariante(&cuentas).await?;
    // 2 USDC: release parcial (1,5) y despues refund del resto.
    e.call_ok(
        &e.capturer,
        "release",
        json!({ "payment_id": ids[0], "amount": "1500000" }),
    )
    .await?;
    e.call_ok(&e.capturer, "refund", json!({ "payment_id": ids[0] }))
        .await?;
    // 1 USDC: release total.
    e.call_ok(&e.capturer, "release", json!({ "payment_id": ids[1] }))
        .await?;
    // 3 USDC: el worker disputa; el arbitro le da 1 y 2 vuelven.
    e.call_ok(
        &e.worker,
        "open_dispute",
        json!({ "payment_id": ids[2], "reason_hash": RAZON }),
    )
    .await?;
    e.call_ok(
        &e.arbiter,
        "resolve_dispute",
        json!({ "payment_id": ids[2], "to_receiver": "1000000", "to_payer": "2000000" }),
    )
    .await?;
    // 4 USDC directos: release total.
    e.call_ok(&e.capturer, "release", json!({ "payment_id": directo }))
        .await?;
    e.invariante(&cuentas).await?;
    // Presupuesto: se agoto (6 de 6); revocar no devuelve nada mas.
    e.call_ok(
        &e.payer,
        "revoke_agent",
        json!({ "agent": agent.id(), "operator_id": "em" }),
    )
    .await?;
    e.invariante(&cuentas).await?;
    for who in cuentas {
        let r = e.withdraw_for(who, Gas::from_tgas(50)).await?;
        assert_eq!(withdrawn_ok(&r), vec![true]);
        e.invariante(&cuentas).await?;
    }
    // Liberado al worker: 1,5 + 1 + 1 + 4 = 7,5 USDC; tarifa 13 %: 0,975; al worker 6,525; vuelve al payer 0,5 + 2.
    assert_eq!(e.ft_balance(e.fees.id()).await?, 975_000);
    assert_eq!(e.ft_balance(e.worker.id()).await?, 6_525_000);
    assert_eq!(
        e.ft_balance(e.payer.id()).await?,
        100 * USDC - 10 * USDC + 2_500_000
    );
    assert_eq!(e.ft_balance(e.arras.id()).await?, 0);
    let cfg = e.config().await?;
    assert_eq!(
        (cfg["tvl_global"].as_str(), cfg["budgets_total"].as_str()),
        (Some("0"), Some("0"))
    );
    Ok(())
}
