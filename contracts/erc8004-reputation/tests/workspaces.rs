//! El ciclo entero del port de ERC-8004 en el sandbox de near-workspaces: Identity, Reputation, el verificador de
//! Arras y Validation (BORRADOR), con **pagos de Arras v1 de verdad** (`contracts/escrow-v1` compilada y la wasm EXACTA
//! de `v1.arras-escrow.near`, `contracts/registry-v1/tests/fixtures`, con su sha256 verificado), el FT de prueba y un
//! receptor de NFT de mentira (`contracts/test-nft-receiver`). Sin red: `NEAR_SANDBOX_BIN_PATH` -> near-sandbox 2.13.4.
//!
//! Las mediciones salen con prefijo `MEDIDA`: `cargo test -p erc8004-reputation --test workspaces -- --nocapture`.

use near_workspaces::network::Sandbox;
use near_workspaces::operations::Function;
use near_workspaces::result::ExecutionFinalResult;
use near_workspaces::types::{CryptoHash, Gas, NearToken};
use near_workspaces::{Account, AccountId, Contract, ContractState, Worker};
use serde_json::{Value, json};
use sha2::Digest;
use tokio::sync::{OnceCell, Semaphore};

static WASM_ID: OnceCell<Vec<u8>> = OnceCell::const_new();
static WASM_REP: OnceCell<Vec<u8>> = OnceCell::const_new();
static WASM_VER: OnceCell<Vec<u8>> = OnceCell::const_new();
static WASM_VAL: OnceCell<Vec<u8>> = OnceCell::const_new();
static WASM_V1: OnceCell<Vec<u8>> = OnceCell::const_new();
static WASM_V1_MAINNET: OnceCell<Vec<u8>> = OnceCell::const_new();
static WASM_FT: OnceCell<Vec<u8>> = OnceCell::const_new();
static WASM_RECV: OnceCell<Vec<u8>> = OnceCell::const_new();
static SANDBOXES: Semaphore = Semaphore::const_new(3);

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

const V1_MAINNET_WASM: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../registry-v1/tests/fixtures/escrow-v1-mainnet-7058319d.wasm"
);
const V1_MAINNET_SHA256: &str = "7058319dff5f8d1e9da91a344ab3c699b709783e4153f3a7da6c2556cde0eada";
const V1_MAINNET_CODE_HASH: &str = "8ZYfMJerz9r1mh4vZH55vyyJb1SLyX7qR2xqpYPxHr5X";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum V1 {
    Compilada,
    Mainnet,
}

async fn wasm_v1(v1: V1) -> anyhow::Result<&'static [u8]> {
    match v1 {
        V1::Compilada => wasm(&WASM_V1, "../escrow-v1").await,
        V1::Mainnet => {
            let w = WASM_V1_MAINNET
                .get_or_try_init(|| async {
                    let bytes = std::fs::read(V1_MAINNET_WASM)?;
                    let sha: [u8; 32] = sha2::Sha256::digest(&bytes).into();
                    let hex: String = sha.iter().map(|b| format!("{b:02x}")).collect();
                    anyhow::ensure!(
                        hex == V1_MAINNET_SHA256,
                        "la wasm de v1 de mainnet no es la esperada: {hex}"
                    );
                    let code_hash: CryptoHash = V1_MAINNET_CODE_HASH.parse()?;
                    anyhow::ensure!(code_hash.0 == sha, "el code hash no es el sha256");
                    Ok(bytes)
                })
                .await?;
            Ok(w.as_slice())
        }
    }
}

fn tg(g: Gas) -> f64 {
    g.as_gas() as f64 / 1e12
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
async fn deploy_init(
    root: &Account,
    name: &str,
    code: &[u8],
    args: Value,
) -> anyhow::Result<Account> {
    let acc = sub(root, name, 10).await?;
    let r = acc
        .batch(acc.id())
        .deploy(code)
        .call(Function::new("new").args_json(args).gas(Gas::from_tgas(30)))
        .transact()
        .await?;
    assert!(
        r.is_success(),
        "deploy + new de {name}: {:?}",
        r.into_result()
    );
    Ok(acc)
}

async fn call_ok(
    from: &Account,
    to: &AccountId,
    method: &str,
    args: Value,
    dep: NearToken,
) -> anyhow::Result<ExecutionFinalResult> {
    let r = from
        .call(to, method)
        .args_json(args)
        .deposit(dep)
        .gas(Gas::from_tgas(150))
        .transact()
        .await?;
    assert!(r.is_success(), "{method}: {:?}", r.clone().into_result());
    Ok(r)
}

fn events(r: &ExecutionFinalResult, standard: &str, event: &str) -> Vec<Value> {
    r.logs()
        .iter()
        .filter_map(|l| l.strip_prefix("EVENT_JSON:"))
        .filter_map(|j| serde_json::from_str::<Value>(j).ok())
        .filter(|e| e["standard"] == standard && e["event"] == event)
        .map(|e| e["data"][0].clone())
        .collect()
}

fn rechazos(r: &ExecutionFinalResult) -> Vec<String> {
    r.logs()
        .iter()
        .filter_map(|l| l.strip_prefix("erc8004:rechazo:"))
        .map(String::from)
        .collect()
}

fn panic_de(r: ExecutionFinalResult) -> String {
    format!("{:?}", r.into_result().unwrap_err())
}

struct Env {
    w: Worker<Sandbox>,
    root: Account,
    ft: Account,
    arras: Account,
    capturer: Account,
    payer: Account,
    worker: Account,
    em: Account,
    tercero: Account,
    identity: Account,
    reputation: Account,
    verifier: Account,
    now: u64,
    register_deposit: u128,
    feedback_deposit: u128,
    proof_deposit: u128,
}

async fn setup() -> anyhow::Result<Env> {
    setup_con(V1::Compilada).await
}

async fn setup_con(v1: V1) -> anyhow::Result<Env> {
    let w = near_workspaces::sandbox().await?;
    let root = w.root_account()?;
    let ft = sub(&root, "ft", 20).await?;
    assert!(
        ft.deploy(wasm(&WASM_FT, "../test-ft").await?)
            .await?
            .is_success()
    );
    let _ = call_ok(
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
    let payer = sub(&root, "payer", 30).await?;
    let worker = sub(&root, "worker", 30).await?;
    let em = sub(&root, "em", 30).await?;
    let tercero = sub(&root, "tercero", 30).await?;
    let arras = sub(&root, "arras", 10).await?;
    let r = arras
        .batch(arras.id())
        .deploy(wasm_v1(v1).await?)
        .call(
            Function::new("new")
                .args_json(json!({
                    "owner": owner.id(), "guardian": owner.id(), "token": ft.id(),
                    "tope_por_pago": (10 * USDC).to_string(), "tope_tvl_global": (1_000 * USDC).to_string(),
                }))
                .gas(Gas::from_tgas(30)),
        )
        .transact()
        .await?;
    assert!(r.is_success(), "deploy v1: {:?}", r.into_result());
    if v1 == V1::Mainnet {
        let hash: CryptoHash = V1_MAINNET_CODE_HASH.parse()?;
        assert_eq!(
            w.view_account(arras.id()).await?.contract_state,
            ContractState::LocalHash(hash)
        );
    }
    for who in [arras.id(), payer.id(), worker.id(), fees.id()] {
        let _ = call_ok(
            &root,
            ft.id(),
            "storage_deposit",
            json!({ "account_id": who, "registration_only": true }),
            NearToken::from_millinear(13),
        )
        .await?;
    }
    let _ = call_ok(
        &ft,
        ft.id(),
        "ft_transfer",
        json!({ "receiver_id": payer.id(), "amount": (100 * USDC).to_string() }),
        ONE_YOCTO,
    )
    .await?;
    let _ = call_ok(
        &owner,
        arras.id(),
        "register_operator",
        json!({ "operator_id": "em", "input": {
            "owner": owner.id(), "capturers": [capturer.id()], "fee_bps": 1300, "fee_recipient": fees.id(),
            "min_amount": "10000", "tope_tvl_operador": (200 * USDC).to_string(),
            "tope_tvl_por_payer": (50 * USDC).to_string(), "sponsor_max_por_payer": "0",
        }}),
        ONE_YOCTO,
    )
    .await?;
    let _ = call_ok(
        &payer,
        arras.id(),
        "storage_deposit",
        json!({}),
        NearToken::from_millinear(500),
    )
    .await?;
    let identity = deploy_init(
        &root,
        "identity",
        wasm(&WASM_ID, "../erc8004-identity").await?,
        json!({}),
    )
    .await?;
    let reputation = deploy_init(
        &root,
        "reputation",
        wasm(&WASM_REP, "./").await?,
        json!({ "identity_registry": identity.id() }),
    )
    .await?;
    let verifier = deploy_init(
        &root,
        "verifier",
        wasm(&WASM_VER, "../erc8004-arras-verifier").await?,
        json!({ "escrow": arras.id() }),
    )
    .await?;
    let icfg: Value = root.view(identity.id(), "get_config").await?.json()?;
    let rcfg: Value = root.view(reputation.id(), "get_config").await?.json()?;
    let now = w.view_block().await?.timestamp();
    Ok(Env {
        register_deposit: icfg["register_deposit"].as_str().unwrap().parse()?,
        feedback_deposit: rcfg["feedback_deposit"].as_str().unwrap().parse()?,
        proof_deposit: rcfg["feedback_proof_deposit"].as_str().unwrap().parse()?,
        w,
        root,
        ft,
        arras,
        capturer,
        payer,
        worker,
        em,
        tercero,
        identity,
        reputation,
        verifier,
        now,
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

impl Env {
    async fn settle(&self) -> anyhow::Result<()> {
        self.w.fast_forward(3).await?;
        Ok(())
    }

    async fn balance(&self, a: &AccountId) -> anyhow::Result<u128> {
        Ok(self.w.view_account(a).await?.balance.as_yoctonear())
    }

    async fn storage(&self, a: &AccountId) -> anyhow::Result<u64> {
        Ok(self.w.view_account(a).await?.storage_usage)
    }

    /// Saldo libre (saldo menos lo que cuesta su storage).
    async fn free(&self, a: &AccountId) -> anyhow::Result<i128> {
        let v = self.w.view_account(a).await?;
        Ok(v.balance.as_yoctonear() as i128 - (u128::from(v.storage_usage) * BYTE) as i128)
    }

    async fn view(&self, c: &Account, method: &str, args: Value) -> anyhow::Result<Value> {
        Ok(self
            .root
            .view(c.id(), method)
            .args_json(args)
            .await?
            .json()?)
    }

    /// authorize real (payer -> receiver) por `ft_transfer_call`; devuelve el `payment_id`.
    async fn authorize(
        &self,
        from: &Account,
        receiver: &AccountId,
        salt: u8,
    ) -> anyhow::Result<String> {
        let msg = msg_for(receiver, self.now + DAY_NS, self.now + 2 * DAY_NS, salt);
        let r = from
            .call(self.ft.id(), "ft_transfer_call")
            .args_json(
                json!({ "receiver_id": self.arras.id(), "amount": USDC.to_string(), "msg": msg }),
            )
            .deposit(ONE_YOCTO)
            .gas(Gas::from_tgas(100))
            .transact()
            .await?;
        let used: String = r.clone().json()?;
        assert_eq!(
            used,
            USDC.to_string(),
            "authorize rechazado: {:?}",
            r.logs()
        );
        Ok(self
            .root
            .view(self.arras.id(), "compute_payment_id")
            .args_json(json!({ "sender_id": from.id(), "amount": USDC.to_string(), "msg": msg }))
            .await?
            .json()?)
    }

    async fn release(&self, id: &str) -> anyhow::Result<()> {
        let _ = call_ok(
            &self.capturer,
            self.arras.id(),
            "release",
            json!({ "payment_id": id, "amount": null }),
            NearToken::from_yoctonear(0),
        )
        .await?;
        Ok(())
    }

    async fn register(
        &self,
        who: &Account,
        uri: &str,
    ) -> anyhow::Result<(String, ExecutionFinalResult)> {
        let r = who
            .call(self.identity.id(), "register")
            .args_json(json!({ "agent_uri": uri }))
            .deposit(NearToken::from_yoctonear(self.register_deposit))
            .gas(Gas::from_tgas(30))
            .transact()
            .await?;
        assert!(r.is_success(), "register: {:?}", r.clone().into_result());
        Ok((r.clone().json()?, r))
    }

    async fn transfer(
        &self,
        from: &Account,
        to: &AccountId,
        id: &str,
    ) -> anyhow::Result<ExecutionFinalResult> {
        let r = from
            .call(self.identity.id(), "nft_transfer")
            .args_json(json!({ "receiver_id": to, "token_id": id }))
            .deposit(ONE_YOCTO)
            .gas(Gas::from_tgas(30))
            .transact()
            .await?;
        assert!(
            r.is_success(),
            "nft_transfer: {:?}",
            r.clone().into_result()
        );
        Ok(r)
    }

    async fn give(
        &self,
        client: &Account,
        agent_id: &str,
        value: &str,
        gas: Gas,
    ) -> anyhow::Result<ExecutionFinalResult> {
        Ok(client
            .call(self.reputation.id(), "give_feedback")
            .args_json(json!({
                "agent_id": agent_id, "value": value, "value_decimals": 0, "tag1": "starred",
                "endpoint": "https://agente.example/mcp", "feedback_uri": "ipfs://bafyfeedback", "feedback_hash": HASH,
            }))
            .deposit(NearToken::from_yoctonear(self.feedback_deposit))
            .gas(gas)
            .transact()
            .await?)
    }

    async fn give_proof(
        &self,
        client: &Account,
        agent_id: &str,
        proof: &str,
        gas: Gas,
    ) -> anyhow::Result<ExecutionFinalResult> {
        Ok(client
            .call(self.reputation.id(), "give_feedback_with_proof")
            .args_json(json!({
                "agent_id": agent_id, "value": "100", "value_decimals": 0, "tag1": "starred", "tag2": "em",
                "verifier": self.verifier.id(), "proof": proof,
            }))
            .deposit(NearToken::from_yoctonear(self.proof_deposit))
            .gas(gas)
            .transact()
            .await?)
    }

    /// Una tx que se rechaza en el callback: devuelve `null`, el log del codigo, y quien llamo solo gasto gas (el
    /// deposito vuelve entero).
    async fn rechazo(
        &self,
        client: &Account,
        r: ExecutionFinalResult,
        b0: u128,
        code: &str,
    ) -> anyhow::Result<()> {
        assert!(r.is_success(), "{:?}", r.clone().into_result());
        assert_eq!(r.clone().json::<Value>()?, Value::Null);
        assert_eq!(rechazos(&r), vec![code.to_string()], "{:?}", r.logs());
        self.settle().await?;
        let gasto = b0 - self.balance(client.id()).await?;
        let quemado = r.total_gas_burnt.as_gas() as u128 * 100_000_000;
        assert!(
            gasto <= quemado + 10u128.pow(21),
            "gasto {gasto} > gas quemado {quemado}: el deposito no volvio"
        );
        Ok(())
    }
}

/// WE-00: deploy + `new` en una tx para cada uno; `new` ajeno rechazado (privado), una sola vez; sin llaves no hay
/// redeploy y el contrato sigue contestando. El ensayo del plan de deploy.
#[tokio::test]
async fn we00_new_privado_una_vez_y_sin_llaves() -> anyhow::Result<()> {
    let _p = SANDBOXES.acquire().await?;
    let w = near_workspaces::sandbox().await?;
    let root = w.root_account()?;
    let ajeno = w.dev_create_account().await?;
    for (code, args) in [
        (wasm(&WASM_ID, "../erc8004-identity").await?, json!({})),
        (
            wasm(&WASM_REP, "./").await?,
            json!({ "identity_registry": "identity.test.near" }),
        ),
        (
            wasm(&WASM_VER, "../erc8004-arras-verifier").await?,
            json!({ "escrow": "arras.test.near" }),
        ),
        (
            wasm(&WASM_VAL, "../erc8004-validation").await?,
            json!({ "identity_registry": "identity.test.near" }),
        ),
    ] {
        let c: Contract = w.dev_deploy(code).await?;
        let err = panic_de(
            ajeno
                .call(c.id(), "new")
                .args_json(&args)
                .transact()
                .await?,
        );
        assert!(err.contains("Method new is private"), "{err}");
        assert!(
            c.call("new")
                .args_json(&args)
                .transact()
                .await?
                .is_success()
        );
        assert!(
            c.call("new")
                .args_json(&args)
                .transact()
                .await?
                .is_failure(),
            "una sola vez"
        );
    }
    let id = deploy_init(
        &root,
        "identity",
        wasm(&WASM_ID, "../erc8004-identity").await?,
        json!({}),
    )
    .await?;
    let keys = w.view_access_keys(id.id()).await?;
    assert_eq!(keys.len(), 1);
    assert!(
        id.batch(id.id())
            .delete_key(keys[0].public_key.clone())
            .transact()
            .await?
            .is_success()
    );
    assert!(w.view_access_keys(id.id()).await?.is_empty());
    let r = id
        .batch(id.id())
        .deploy(wasm(&WASM_ID, "../erc8004-identity").await?)
        .transact()
        .await;
    assert!(
        r.is_err() || r.unwrap().is_failure(),
        "sin llaves no hay redeploy"
    );
    let v: String = root.view(id.id(), "get_version").await?.json()?;
    assert_eq!(v, "2.0.0");
    for (name, code) in [
        ("identity", wasm(&WASM_ID, "../erc8004-identity").await?),
        ("reputation", wasm(&WASM_REP, "./").await?),
        (
            "verifier",
            wasm(&WASM_VER, "../erc8004-arras-verifier").await?,
        ),
        (
            "validation",
            wasm(&WASM_VAL, "../erc8004-validation").await?,
        ),
    ] {
        println!(
            "MEDIDA WE-00 wasm {name}: {} bytes ({:.4} NEAR de storage)",
            code.len(),
            code.len() as f64 / 1e5
        );
    }
    Ok(())
}

/// WE-01: el alta del worker por los dos caminos (RESPUESTA 2 de c0der): hoy EM paga (`register` + `nft_transfer`,
/// el worker queda dueno y no firma nada); despues el worker firma su `register`. Mide deposito, gas y tx.
#[tokio::test]
async fn we01_alta_del_worker_por_los_dos_caminos() -> anyhow::Result<()> {
    let _p = SANDBOXES.acquire().await?;
    let e = setup().await?;
    let uri = "https://em.example/workers/worker.json";
    // Camino 1: EM paga.
    e.settle().await?;
    let b0 = e.balance(e.em.id()).await?;
    let s0 = e.storage(e.identity.id()).await?;
    let (id, r1) = e.register(&e.em, uri).await?;
    let ev = events(&r1, "erc8004", "metadata_set");
    assert_eq!(ev[0]["metadata_key"], "agentWallet");
    let r2 = e.transfer(&e.em, e.worker.id(), &id).await?;
    e.settle().await?;
    let gasto_em = b0 - e.balance(e.em.id()).await?;
    let bytes = e.storage(e.identity.id()).await? - s0;
    let tok = e
        .view(&e.identity, "nft_token", json!({ "token_id": id }))
        .await?;
    assert_eq!(tok["owner_id"], e.worker.id().as_str());
    assert_eq!(tok["metadata"]["reference"], uri);
    assert_eq!(
        e.view(&e.identity, "get_agent_wallet", json!({ "agent_id": id }))
            .await?,
        Value::Null
    );
    assert_eq!(
        events(&r2, "nep171", "nft_transfer")[0]["new_owner_id"],
        e.worker.id().as_str()
    );
    println!(
        "MEDIDA WE-01 camino EM: 2 tx de EM (register {:.3} TGas + nft_transfer {:.3} TGas), EM gasto {:.5} NEAR, \
         el registro crecio {bytes} bytes (+{} de reserva); el worker no firmo nada",
        tg(r1.total_gas_burnt),
        tg(r2.total_gas_burnt),
        gasto_em as f64 / 1e24,
        77
    );
    // El worker puede poner su agentWallet despues (opcional): 1 tx suya.
    let r3 = call_ok(
        &e.worker,
        e.identity.id(),
        "set_agent_wallet",
        json!({ "agent_id": id, "new_wallet": e.worker.id(), "deadline": (e.now + 60 * 1_000_000_000).to_string() }),
        ONE_YOCTO,
    )
    .await;
    // El `now` del setup ya puede ser viejo: se recalcula si hace falta.
    let r3 = match r3 {
        Ok(r) => r,
        Err(_) => {
            let now = e.w.view_block().await?.timestamp();
            call_ok(&e.worker, e.identity.id(), "set_agent_wallet", json!({ "agent_id": id, "new_wallet": e.worker.id(), "deadline": (now + 120 * 1_000_000_000).to_string() }), ONE_YOCTO).await?
        }
    };
    assert_eq!(
        e.view(&e.identity, "get_agent_wallet", json!({ "agent_id": id }))
            .await?,
        json!(e.worker.id())
    );
    println!(
        "MEDIDA WE-01 set_agent_wallet del worker a si mismo: 1 tx, 1 yocto, {:.3} TGas",
        tg(r3.total_gas_burnt)
    );
    // Camino 2: el worker firma su propia alta.
    e.settle().await?;
    let b0 = e.balance(e.tercero.id()).await?;
    let (id2, r4) = e.register(&e.tercero, uri).await?;
    e.settle().await?;
    let gasto = b0 - e.balance(e.tercero.id()).await?;
    assert_eq!(
        e.view(&e.identity, "get_agent_wallet", json!({ "agent_id": id2 }))
            .await?,
        json!(e.tercero.id())
    );
    println!(
        "MEDIDA WE-01 camino worker: 1 tx (register {:.3} TGas), gasto {:.5} NEAR",
        tg(r4.total_gas_burnt),
        gasto as f64 / 1e24
    );
    Ok(())
}

/// WE-02: agentWallet en dos pasos con cuentas reales, y la transferencia que la limpia.
#[tokio::test]
async fn we02_agent_wallet_y_transferencia() -> anyhow::Result<()> {
    let _p = SANDBOXES.acquire().await?;
    let e = setup().await?;
    let (id, _) = e.register(&e.worker, "ipfs://bafyagente").await?;
    let now = e.w.view_block().await?.timestamp();
    let r = call_ok(&e.worker, e.identity.id(), "set_agent_wallet", json!({ "agent_id": id, "new_wallet": e.em.id(), "deadline": (now + 200 * 1_000_000_000).to_string() }), ONE_YOCTO).await?;
    assert!(!r.json::<bool>()?);
    let pend = e
        .view(
            &e.identity,
            "get_pending_agent_wallet",
            json!({ "agent_id": id }),
        )
        .await?;
    assert_eq!(pend["wallet"], e.em.id().as_str());
    // Otro no puede aceptar por la wallet.
    let err = panic_de(
        e.tercero
            .call(e.identity.id(), "accept_agent_wallet")
            .args_json(json!({ "agent_id": id }))
            .deposit(ONE_YOCTO)
            .transact()
            .await?,
    );
    assert!(err.contains("ERC8004_ID_E17"), "{err}");
    let r = call_ok(
        &e.em,
        e.identity.id(),
        "accept_agent_wallet",
        json!({ "agent_id": id }),
        ONE_YOCTO,
    )
    .await?;
    assert_eq!(
        events(&r, "erc8004", "metadata_set")[0]["metadata_key"],
        "agentWallet"
    );
    assert_eq!(
        e.view(&e.identity, "get_agent_wallet", json!({ "agent_id": id }))
            .await?,
        json!(e.em.id())
    );
    let r = e.transfer(&e.worker, e.tercero.id(), &id).await?;
    assert_eq!(
        events(&r, "erc8004", "metadata_set")[0]["metadata_value"],
        ""
    );
    assert_eq!(
        e.view(&e.identity, "get_agent_wallet", json!({ "agent_id": id }))
            .await?,
        Value::Null
    );
    let chk = e
        .view(
            &e.identity,
            "check_agent",
            json!({ "agent_id": id, "account_id": e.worker.id() }),
        )
        .await?;
    assert_eq!(
        chk,
        json!({ "owner_id": e.tercero.id(), "agent_wallet": null, "authorized": false })
    );
    Ok(())
}

/// WE-03: feedback del nucleo: un cliente califica; el dueno, un aprobado y un agente inexistente se rechazan en el
/// callback con el deposito de vuelta; un Identity Registry sin codigo es `E20`.
#[tokio::test]
async fn we03_feedback_del_nucleo_y_cada_rechazo() -> anyhow::Result<()> {
    let _p = SANDBOXES.acquire().await?;
    let e = setup().await?;
    let (id, _) = e.register(&e.worker, "ipfs://bafyagente").await?;
    let _ = call_ok(
        &e.worker,
        e.identity.id(),
        "nft_approve",
        json!({ "token_id": id, "account_id": e.em.id() }),
        NearToken::from_millinear(10),
    )
    .await?;
    e.settle().await?;
    let f0 = e.free(e.reputation.id()).await?;
    let s0 = e.storage(e.reputation.id()).await?;
    let r = e.give(&e.payer, &id, "87", Gas::from_tgas(100)).await?;
    assert!(r.is_success(), "{:?}", r.clone().into_result());
    assert_eq!(r.clone().json::<Value>()?, json!("1"));
    let ev = events(&r, "erc8004", "new_feedback");
    assert_eq!(ev[0]["client_address"], e.payer.id().as_str());
    assert_eq!(ev[0]["feedback_hash"], HASH);
    e.settle().await?;
    let bytes = e.storage(e.reputation.id()).await? - s0;
    assert!(
        e.free(e.reputation.id()).await? >= f0,
        "el registro no subsidia"
    );
    let por_recibo: Vec<String> = r
        .outcomes()
        .iter()
        .map(|o| format!("{} {:.3}", o.executor_id, tg(o.gas_burnt)))
        .collect();
    println!(
        "MEDIDA WE-03 give_feedback aceptada: {:.3} TGas total, {bytes} bytes; recibos {por_recibo:?}",
        tg(r.total_gas_burnt)
    );
    for (who, code) in [
        (&e.worker, "E23_AUTO_CALIFICACION"),
        (&e.em, "E23_AUTO_CALIFICACION"),
    ] {
        e.settle().await?;
        let b0 = e.balance(who.id()).await?;
        let r = e.give(who, &id, "100", Gas::from_tgas(100)).await?;
        e.rechazo(who, r, b0, code).await?;
    }
    e.settle().await?;
    let b0 = e.balance(e.payer.id()).await?;
    let r = e.give(&e.payer, "99", "1", Gas::from_tgas(100)).await?;
    println!(
        "MEDIDA WE-03 give_feedback rechazada (E22): {:.3} TGas",
        tg(r.total_gas_burnt)
    );
    e.rechazo(&e.payer, r, b0, "E22_AGENTE_INEXISTENTE").await?;
    // Un Reputation Registry que apunta a una cuenta sin codigo: la vista falla de verdad.
    let nocode = sub(&e.root, "nocode", 1).await?;
    let rep2 = deploy_init(
        &e.root,
        "rep2",
        wasm(&WASM_REP, "./").await?,
        json!({ "identity_registry": nocode.id() }),
    )
    .await?;
    e.settle().await?;
    let b0 = e.balance(e.payer.id()).await?;
    let r = e
        .payer
        .call(rep2.id(), "give_feedback")
        .args_json(json!({ "agent_id": "0", "value": "1", "value_decimals": 0 }))
        .deposit(NearToken::from_yoctonear(e.feedback_deposit))
        .gas(Gas::from_tgas(100))
        .transact()
        .await?;
    e.rechazo(&e.payer, r, b0, "E20_VISTA_DE_IDENTIDAD_FALLO")
        .await?;
    // Las guardas sincronicas entran en panico (el runtime devuelve el deposito).
    let err = panic_de(
        e.payer
            .call(e.reputation.id(), "give_feedback")
            .args_json(json!({ "agent_id": id, "value": "1", "value_decimals": 19 }))
            .deposit(NearToken::from_yoctonear(e.feedback_deposit))
            .gas(Gas::from_tgas(100))
            .transact()
            .await?,
    );
    assert!(err.contains("ERC8004_REP_E04: too many decimals"), "{err}");
    Ok(())
}

/// WE-04: revocar, `append_response` de cualquiera, y el resumen con filtro de clientes.
#[tokio::test]
async fn we04_revocar_responder_y_resumir() -> anyhow::Result<()> {
    let _p = SANDBOXES.acquire().await?;
    let e = setup().await?;
    let (id, _) = e.register(&e.worker, "ipfs://bafyagente").await?;
    assert!(
        e.give(&e.payer, &id, "80", Gas::from_tgas(100))
            .await?
            .is_success()
    );
    assert!(
        e.give(&e.payer, &id, "90", Gas::from_tgas(100))
            .await?
            .is_success()
    );
    assert!(
        e.give(&e.tercero, &id, "10", Gas::from_tgas(100))
            .await?
            .is_success()
    );
    let s = e
        .view(
            &e.reputation,
            "get_summary",
            json!({ "agent_id": id, "client_addresses": [e.payer.id()] }),
        )
        .await?;
    assert_eq!(
        s,
        json!({ "count": "2", "summary_value": "85", "summary_value_decimals": 0 })
    );
    let s = e
        .view(
            &e.reputation,
            "get_summary",
            json!({ "agent_id": id, "client_addresses": [e.payer.id(), e.tercero.id()] }),
        )
        .await?;
    assert_eq!(s["summary_value"], "60");
    let err = e
        .root
        .view(e.reputation.id(), "get_summary")
        .args_json(json!({ "agent_id": id, "client_addresses": [] }))
        .await;
    assert!(
        format!("{err:?}").contains("clientAddresses required"),
        "{err:?}"
    );
    let r = call_ok(
        &e.payer,
        e.reputation.id(),
        "revoke_feedback",
        json!({ "agent_id": id, "feedback_index": "2" }),
        ONE_YOCTO,
    )
    .await?;
    assert_eq!(events(&r, "erc8004", "feedback_revoked").len(), 1);
    let s = e
        .view(
            &e.reputation,
            "get_summary",
            json!({ "agent_id": id, "client_addresses": [e.payer.id()] }),
        )
        .await?;
    assert_eq!(s["summary_value"], "80");
    // El agente (su dueno) responde, y tambien un tercero.
    let r = call_ok(&e.worker, e.reputation.id(), "append_response", json!({ "agent_id": id, "client_address": e.tercero.id(), "feedback_index": "1", "response_uri": "ipfs://bafyrefund" }), NearToken::from_millinear(10)).await?;
    assert_eq!(
        events(&r, "erc8004", "response_appended")[0]["responder"],
        e.worker.id().as_str()
    );
    let _ = call_ok(&e.em, e.reputation.id(), "append_response", json!({ "agent_id": id, "client_address": e.tercero.id(), "feedback_index": "1", "response_uri": "https://em.example/spam" }), NearToken::from_millinear(10)).await?;
    let n = e
        .view(
            &e.reputation,
            "get_response_count",
            json!({ "agent_id": id }),
        )
        .await?;
    assert_eq!(n, json!("2"));
    let all = e
        .view(
            &e.reputation,
            "read_all_feedback",
            json!({ "agent_id": id, "include_revoked": true }),
        )
        .await?;
    assert_eq!(all["revoked_statuses"], json!([false, true, false]));
    let clients = e
        .view(&e.reputation, "get_clients", json!({ "agent_id": id }))
        .await?;
    assert_eq!(clients, json!([e.payer.id(), e.tercero.id()]));
    println!(
        "MEDIDA WE-04 append_response (respondedor nuevo): {:.3} TGas",
        tg(r.total_gas_burnt)
    );
    Ok(())
}

/// WE-05: feedback con prueba sobre pagos `Released` reales de Arras v1 (compilada y la de mainnet). El payer califica
/// al agente del worker (el worker es su dueno) y el worker al del payer; un tercero, un pago sin liberar, una prueba
/// repetida y una contraparte que no es el agente se rechazan.
async fn we05(v1: V1) -> anyhow::Result<()> {
    let e = setup_con(v1).await?;
    let (agente_worker, _) = e
        .register(&e.em, "https://em.example/workers/w.json")
        .await?;
    let _ = e.transfer(&e.em, e.worker.id(), &agente_worker).await?;
    let (agente_payer, _) = e
        .register(&e.payer, "https://agente.example/a.json")
        .await?;
    let pid = e.authorize(&e.payer, e.worker.id(), 1).await?;
    e.release(&pid).await?;
    let pago = e
        .view(&e.arras, "get_payment", json!({ "payment_id": pid }))
        .await?;
    assert_eq!(pago["status"], "Released");
    // El payer califica al agente del worker.
    e.settle().await?;
    let f0 = e.free(e.reputation.id()).await?;
    let r = e
        .give_proof(&e.payer, &agente_worker, &pid, Gas::from_tgas(150))
        .await?;
    assert!(r.is_success(), "{:?}", r.clone().into_result());
    assert_eq!(r.clone().json::<Value>()?, json!("1"), "{:?}", r.logs());
    let ev = events(&r, "erc8004", "new_feedback");
    assert_eq!(
        ev[0]["proof"],
        json!({ "verifier": e.verifier.id(), "proof": pid, "counterparty": e.worker.id(), "role": "receiver",
                "context": "em", "weight": pago["released"] })
    );
    e.settle().await?;
    assert!(e.free(e.reputation.id()).await? >= f0);
    let por_recibo: Vec<String> = r
        .outcomes()
        .iter()
        .map(|o| format!("{} {:.3}", o.executor_id, tg(o.gas_burnt)))
        .collect();
    println!(
        "MEDIDA WE-05 ({v1:?}) give_feedback_with_proof aceptada: {:.3} TGas total; recibos {por_recibo:?}",
        tg(r.total_gas_burnt)
    );
    // El worker califica al agente del payer (rol de la contraparte: payer).
    let r = e
        .give_proof(&e.worker, &agente_payer, &pid, Gas::from_tgas(150))
        .await?;
    assert_eq!(r.clone().json::<Value>()?, json!("1"), "{:?}", r.logs());
    assert_eq!(
        events(&r, "erc8004", "new_feedback")[0]["proof"]["role"],
        "payer"
    );
    let v = e
        .view(
            &e.reputation,
            "get_verified_summary",
            json!({ "agent_id": agente_worker, "verifier": e.verifier.id(), "context": "em" }),
        )
        .await?;
    assert_eq!(v["count"], "1");
    assert_eq!(v["weight_sum"], pago["released"]);
    // Una prueba repetida: panic sincronico.
    let err = panic_de(
        e.give_proof(&e.payer, &agente_worker, &pid, Gas::from_tgas(150))
            .await?,
    );
    assert!(err.contains("ERC8004_REP_E42"), "{err}");
    // Un tercero que no es parte del pago: el verificador entra en panico (E15) y el registro rechaza con E45.
    e.settle().await?;
    let b0 = e.balance(e.tercero.id()).await?;
    let r = e
        .give_proof(&e.tercero, &agente_worker, &pid, Gas::from_tgas(150))
        .await?;
    e.rechazo(&e.tercero, r, b0, "E45_VERIFICADOR_FALLO")
        .await?;
    // Un pago autorizado sin liberar.
    let pid2 = e.authorize(&e.payer, e.worker.id(), 2).await?;
    e.settle().await?;
    let b0 = e.balance(e.payer.id()).await?;
    let r = e
        .give_proof(&e.payer, &agente_worker, &pid2, Gas::from_tgas(150))
        .await?;
    e.rechazo(&e.payer, r, b0, "E45_VERIFICADOR_FALLO").await?;
    // Un pago real (otro, liberado) con el worker, pero calificando a un agente que no es del worker: E47. Con el mismo
    // `pid` seria E42 (una calificacion por prueba y cliente, sea al agente que sea).
    let (agente_ajeno, _) = e.register(&e.tercero, "ipfs://bafyajeno").await?;
    let pid3 = e.authorize(&e.payer, e.worker.id(), 3).await?;
    e.release(&pid3).await?;
    let err = panic_de(
        e.give_proof(&e.payer, &agente_ajeno, &pid, Gas::from_tgas(150))
            .await?,
    );
    assert!(err.contains("ERC8004_REP_E42"), "{err}");
    e.settle().await?;
    let b0 = e.balance(e.payer.id()).await?;
    let r = e
        .give_proof(&e.payer, &agente_ajeno, &pid3, Gas::from_tgas(150))
        .await?;
    e.rechazo(&e.payer, r, b0, "E47_NO_ES_EL_AGENTE").await?;
    // La calificacion con prueba tambien cuenta en el resumen del nucleo.
    let s = e
        .view(
            &e.reputation,
            "get_summary",
            json!({ "agent_id": agente_worker, "client_addresses": [e.payer.id()] }),
        )
        .await?;
    assert_eq!(s["count"], "1");
    Ok(())
}

#[tokio::test]
async fn we05_prueba_de_pago_con_v1_compilada() -> anyhow::Result<()> {
    let _p = SANDBOXES.acquire().await?;
    we05(V1::Compilada).await
}

#[tokio::test]
async fn we05m_prueba_de_pago_con_v1_de_mainnet() -> anyhow::Result<()> {
    let _p = SANDBOXES.acquire().await?;
    we05(V1::Mainnet).await
}

/// WE-06: Validation (BORRADOR): el dueno pide, el validador nombrado responde, el resumen.
#[tokio::test]
async fn we06_validacion_borrador() -> anyhow::Result<()> {
    let _p = SANDBOXES.acquire().await?;
    let e = setup().await?;
    let val = deploy_init(
        &e.root,
        "validation",
        wasm(&WASM_VAL, "../erc8004-validation").await?,
        json!({ "identity_registry": e.identity.id() }),
    )
    .await?;
    let cfg: Value = e.view(&val, "get_config", json!({})).await?;
    assert_eq!(cfg["status"], "BORRADOR");
    let dep: u128 = cfg["request_deposit"].as_str().unwrap().parse()?;
    let (id, _) = e.register(&e.worker, "ipfs://bafyagente").await?;
    let req = |who: &Account, h: &str| {
        who.call(val.id(), "validation_request")
            .args_json(json!({ "validator_address": e.em.id(), "agent_id": id, "request_uri": "ipfs://bafypedido", "request_hash": h }))
            .deposit(NearToken::from_yoctonear(dep))
            .gas(Gas::from_tgas(100))
            .transact()
    };
    let r = req(&e.worker, HASH).await?;
    assert!(r.clone().json::<bool>()?, "{:?}", r.logs());
    println!(
        "MEDIDA WE-06 validation_request: {:.3} TGas",
        tg(r.total_gas_burnt)
    );
    let r = req(&e.payer, &"1".repeat(64)).await?;
    assert!(!r.clone().json::<bool>()?);
    assert_eq!(rechazos(&r), vec!["E23_NO_AUTORIZADO".to_string()]);
    let err = panic_de(
        e.payer
            .call(val.id(), "validation_response")
            .args_json(json!({ "request_hash": HASH, "response": 100 }))
            .deposit(ONE_YOCTO)
            .transact()
            .await?,
    );
    assert!(err.contains("not validator"), "{err}");
    let _ = call_ok(
        &e.em,
        val.id(),
        "validation_response",
        json!({ "request_hash": HASH, "response": 100, "tag": "hard" }),
        NearToken::from_millinear(10),
    )
    .await?;
    let s = e
        .view(&val, "get_summary", json!({ "agent_id": id }))
        .await?;
    assert_eq!(s, json!({ "count": "1", "average_response": 100 }));
    Ok(())
}

/// WE-07: los callbacks son privados (`#[private]` lo pone el wrapper de la wasm).
#[tokio::test]
async fn we07_callbacks_privados() -> anyhow::Result<()> {
    let _p = SANDBOXES.acquire().await?;
    let e = setup().await?;
    let val = deploy_init(
        &e.root,
        "validation",
        wasm(&WASM_VAL, "../erc8004-validation").await?,
        json!({ "identity_registry": e.identity.id() }),
    )
    .await?;
    let p = json!({ "client": e.payer.id(), "agent_id": "0", "value": "1", "value_decimals": 0, "tag1": "", "tag2": "",
                    "endpoint": "", "feedback_uri": "", "feedback_hash": null, "deposit": "0" });
    for (c, method, args) in [
        (&e.reputation, "on_feedback_checked", json!({ "p": p })),
        (
            &e.reputation,
            "on_feedback_proof_checked",
            json!({ "p": p, "verifier": e.verifier.id(), "proof": "x", "nonce": "0" }),
        ),
        (
            &e.identity,
            "nft_resolve_transfer",
            json!({ "previous_owner_id": e.payer.id(), "receiver_id": e.worker.id(), "token_id": "0", "approved_account_ids": null, "approvals_refund": "0" }),
        ),
        (&e.verifier, "on_payment", json!({ "client": e.payer.id() })),
        (
            &val,
            "on_request_checked",
            json!({ "p": { "requester": e.payer.id(), "validator_address": e.em.id(), "agent_id": "0",
                           "request_uri": "a:b", "request_hash": HASH, "deposit": "0" }, "nonce": "0" }),
        ),
    ] {
        let err = panic_de(
            e.tercero
                .call(c.id(), method)
                .args_json(args)
                .gas(Gas::from_tgas(50))
                .transact()
                .await?,
        );
        assert!(
            err.contains(&format!("Method {method} is private")),
            "{method}: {err}"
        );
    }
    Ok(())
}

/// WE-08: `nft_transfer_call` a un contrato que lo devuelve, que se lo queda, que entra en panico, y a una cuenta sin
/// codigo.
#[tokio::test]
async fn we08_nft_transfer_call() -> anyhow::Result<()> {
    let _p = SANDBOXES.acquire().await?;
    let e = setup().await?;
    // El receptor no tiene `new` (estado por defecto): se despliega solo.
    let recv = sub(&e.root, "receptor", 10).await?;
    assert!(
        recv.deploy(wasm(&WASM_RECV, "../test-nft-receiver").await?)
            .await?
            .is_success()
    );
    let nocode = sub(&e.root, "sincodigo", 1).await?;
    let (id, _) = e.register(&e.worker, "ipfs://bafyagente").await?;
    for (to, msg, queda) in [
        (recv.id(), "return", false),
        (recv.id(), "panic", false),
        (nocode.id(), "x", false),
        (recv.id(), "keep", true),
    ] {
        let r = e
            .worker
            .call(e.identity.id(), "nft_transfer_call")
            .args_json(json!({ "receiver_id": to, "token_id": id, "msg": msg }))
            .deposit(ONE_YOCTO)
            .gas(Gas::from_tgas(100))
            .transact()
            .await?;
        assert!(r.is_success(), "{msg}: {:?}", r.clone().into_result());
        assert_eq!(r.clone().json::<bool>()?, queda, "{msg}");
        let owner = e
            .view(&e.identity, "nft_token", json!({ "token_id": id }))
            .await?["owner_id"]
            .clone();
        let esperado = if queda {
            json!(to)
        } else {
            json!(e.worker.id())
        };
        assert_eq!(owner, esperado, "{msg}");
        println!(
            "MEDIDA WE-08 nft_transfer_call ({msg}): {:.3} TGas",
            tg(r.total_gas_burnt)
        );
    }
    Ok(())
}

/// WE-09: con el gas minimo justo, `give_feedback` y `give_feedback_with_proof` (sobre v1 real) llegan al callback y
/// aceptan.
#[tokio::test]
async fn we09_gas_minimo_justo() -> anyhow::Result<()> {
    let _p = SANDBOXES.acquire().await?;
    let e = setup().await?;
    let rcfg = e.view(&e.reputation, "get_config", json!({})).await?;
    let min: u64 = rcfg["min_gas_give_feedback"].as_str().unwrap().parse()?;
    let min_p: u64 = rcfg["min_gas_give_feedback_with_proof"]
        .as_str()
        .unwrap()
        .parse()?;
    let (id, _) = e.register(&e.worker, "ipfs://bafyagente").await?;
    let r0 = e.give(&e.payer, &id, "1", Gas::from_gas(min)).await?;
    assert_eq!(
        r0.clone().json::<Value>()?,
        json!("1"),
        "{:?}",
        r0.clone().into_result()
    );
    let err = panic_de(e.give(&e.payer, &id, "1", Gas::from_gas(min - 1)).await?);
    assert!(err.contains("ERC8004_REP_E10"), "{err}");
    let pid = e.authorize(&e.payer, e.worker.id(), 9).await?;
    e.release(&pid).await?;
    let r = e
        .give_proof(&e.payer, &id, &pid, Gas::from_gas(min_p))
        .await?;
    assert_eq!(r.clone().json::<Value>()?, json!("2"), "{:?}", r.logs());
    println!(
        "MEDIDA WE-09 con el minimo justo: give_feedback {} TGas -> {:.3} quemados; con prueba {} TGas -> {:.3}",
        min / 1_000_000_000_000,
        tg(r0.total_gas_burnt),
        min_p / 1_000_000_000_000,
        tg(r.total_gas_burnt)
    );
    Ok(())
}

/// WE-10: transferir agentes a cuentas nuevas no baja el saldo libre del Identity Registry (la reserva por token de la
/// seccion 2.4 del plan).
#[tokio::test]
async fn we10_transferir_a_cuentas_nuevas_no_vacia_el_contrato() -> anyhow::Result<()> {
    let _p = SANDBOXES.acquire().await?;
    let e = setup().await?;
    // La base es ANTES de las altas: la reserva se paga en `register` y la consumen las transferencias.
    e.settle().await?;
    let f0 = e.free(e.identity.id()).await?;
    let mut ids = vec![];
    for _ in 0..6 {
        ids.push(e.register(&e.tercero, "a:b").await?.0);
    }
    e.settle().await?;
    let s0 = e.storage(e.identity.id()).await?;
    for (i, id) in ids.iter().enumerate() {
        let nueva: AccountId = format!("{}{i}.test.near", "n".repeat(50)).parse()?;
        let _ = e.transfer(&e.tercero, &nueva, id).await?;
    }
    e.settle().await?;
    let f1 = e.free(e.identity.id()).await?;
    let s1 = e.storage(e.identity.id()).await?;
    println!(
        "MEDIDA WE-10 6 transferencias a cuentas nuevas: storage {s0} -> {s1} bytes, saldo libre {f0} -> {f1}"
    );
    assert!(f1 >= f0, "el saldo libre bajo: {f0} -> {f1}");
    assert!(s1 - s0 <= 6 * 77);
    Ok(())
}

/// WE-11: lo que ocupa cada cuenta recien desplegada (codigo + estado de `new` + la cuenta), con su llave y sin ella:
/// el piso de NEAR de cada cuenta del plan de deploy (1 NEAR por 100 KB).
#[tokio::test]
async fn we11_storage_de_cada_cuenta_recien_desplegada() -> anyhow::Result<()> {
    let _p = SANDBOXES.acquire().await?;
    let w = near_workspaces::sandbox().await?;
    let root = w.root_account()?;
    for (name, code, args) in [
        (
            "identity",
            wasm(&WASM_ID, "../erc8004-identity").await?,
            json!({}),
        ),
        (
            "reputation",
            wasm(&WASM_REP, "./").await?,
            json!({ "identity_registry": "identity.test.near" }),
        ),
        (
            "verifier",
            wasm(&WASM_VER, "../erc8004-arras-verifier").await?,
            json!({ "escrow": "arras.test.near" }),
        ),
        (
            "validation",
            wasm(&WASM_VAL, "../erc8004-validation").await?,
            json!({ "identity_registry": "identity.test.near" }),
        ),
    ] {
        let acc = deploy_init(&root, name, code, args).await?;
        let con_llave = w.view_account(acc.id()).await?.storage_usage;
        let keys = w.view_access_keys(acc.id()).await?;
        assert!(
            acc.batch(acc.id())
                .delete_key(keys[0].public_key.clone())
                .transact()
                .await?
                .is_success()
        );
        let sin_llave = w.view_account(acc.id()).await?.storage_usage;
        assert!(sin_llave < con_llave);
        assert!(sin_llave > code.len() as u64);
        println!(
            "MEDIDA WE-11 {name}: codigo {} bytes; la cuenta ocupa {con_llave} bytes con la llave y {sin_llave} sin ella \
             ({:.5} NEAR bloqueados)",
            code.len(),
            sin_llave as f64 / 1e5
        );
    }
    Ok(())
}
