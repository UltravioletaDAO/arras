//! El verificador contra el escrow de Arras de verdad en el sandbox de near-workspaces: `contracts/escrow-v2`
//! compilada (la misma fuente que la wasm de `v2.arras-escrow.near`: `git diff ddc5121 -- contracts/escrow-v2
//! contracts/escrow-core` esta vacio), el FT de prueba, el escrow de mentira (`contracts/test-escrow`, para las
//! respuestas que el escrow real no da) y, en WP-06, Identity y Reputation. Sin red: `NEAR_SANDBOX_BIN_PATH` ->
//! near-sandbox 2.13.4.
//!
//! - WP-01: liberado total y parcial, refund con y sin liberacion previa, un pago autorizado, el client ajeno.
//! - WP-02: disputa abierta despues de una liberacion parcial y resuelta por el arbitro; abierta sin liberacion y
//!   devuelta entera al payer.
//! - WP-03: disputas vencidas (el sandbox avanza el reloj): la del payer antes de `release_deadline` le da lo vivo al
//!   receiver y no es prueba (`E16`), con y sin liberacion previa; la del receiver se lo devuelve al payer y vale por lo
//!   liberado antes (o `E13`).
//! - WP-04: proof inexistente, no canonica, gas de menos, y un escrow sin codigo (`E10`).
//! - WP-05: el escrow de mentira: un `Payment` real copiado tal cual vale; con un campo de mas, con uno de menos (el de
//!   v1), incoherente, payer == receiver o de mas de 8 KiB, no.
//! - WP-06: el ciclo con el Reputation Registry desplegado (sin tocarlo): `give_feedback_with_proof` con este verificador
//!   sobre un pago del escrow, la prueba guardada y la segunda vez rechazada.
//! - WP-07: medidas (gas y storage para el plan de deploy) y el verificador sin llaves sigue andando.
//!
//! `cargo test -p erc8004-payment-proof --test workspaces -- --nocapture` imprime las `MEDIDA`.

use near_workspaces::network::Sandbox;
use near_workspaces::operations::Function;
use near_workspaces::result::ExecutionFinalResult;
use near_workspaces::types::{Gas, NearToken};
use near_workspaces::{Account, AccountId, Worker};
use serde_json::{Value, json};
use tokio::sync::{OnceCell, Semaphore};

static WASM_V2: OnceCell<Vec<u8>> = OnceCell::const_new();
static WASM_FT: OnceCell<Vec<u8>> = OnceCell::const_new();
static WASM_PP: OnceCell<Vec<u8>> = OnceCell::const_new();
static WASM_TE: OnceCell<Vec<u8>> = OnceCell::const_new();
static WASM_ID: OnceCell<Vec<u8>> = OnceCell::const_new();
static WASM_REP: OnceCell<Vec<u8>> = OnceCell::const_new();
/// Cuantos sandboxes a la vez (cada test levanta el suyo).
static SANDBOXES: Semaphore = Semaphore::const_new(3);

const USDC: u128 = 1_000_000;
const ONE_YOCTO: NearToken = NearToken::from_yoctonear(1);
const ZERO: NearToken = NearToken::from_yoctonear(0);
const DAY_NS: u64 = 86_400 * 1_000_000_000;
const MIN_WINDOW_NS: u64 = 60 * 1_000_000_000;
const RAZON: &str = "2222222222222222222222222222222222222222222222222222222222222222";
/// Un `payment_id` canonico que ningun pago tiene (el de V-1 de v1).
const PID_NADIE: &str = "DN5eMA3sNES7xsoy6qvy24cVF6Dmwjxpkvd2b4e8P38M";

async fn wasm(
    cell: &'static OnceCell<Vec<u8>>,
    path: &'static str,
) -> anyhow::Result<&'static [u8]> {
    let w = cell
        .get_or_try_init(|| async { near_workspaces::compile_project(path).await })
        .await?;
    Ok(w.as_slice())
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

fn panic_de(r: ExecutionFinalResult) -> String {
    format!("{:?}", r.into_result().unwrap_err())
}

fn salt_hex(n: u8) -> String {
    format!("{}{n:02x}", "0".repeat(30))
}

struct Env {
    w: Worker<Sandbox>,
    root: Account,
    ft: Account,
    arras: Account,
    capturer: Account,
    arbiter: Account,
    payer: Account,
    worker: Account,
    tercero: Account,
    verifier: Account,
    now: u64,
}

/// El FT, el escrow con el operador `em` (capturer, arbitro con `arbiter_window_ns`) y el verificador apuntado a el.
async fn setup(arbiter_window_ns: u64) -> anyhow::Result<Env> {
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
        ZERO,
    )
    .await?;
    let owner = sub(&root, "owner", 10).await?;
    let capturer = sub(&root, "capturer", 10).await?;
    let arbiter = sub(&root, "arbiter", 5).await?;
    let fees = sub(&root, "fees", 5).await?;
    let payer = sub(&root, "payer", 30).await?;
    let worker = sub(&root, "worker", 30).await?;
    let tercero = sub(&root, "tercero", 10).await?;
    let arras = deploy_init(
        &root,
        "arras",
        wasm(&WASM_V2, "../escrow-v2").await?,
        json!({
            "owner": owner.id(), "guardian": owner.id(), "token": ft.id(),
            "tope_por_pago": (10 * USDC).to_string(), "tope_tvl_global": (1_000 * USDC).to_string(),
        }),
    )
    .await?;
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
            "arbiter": arbiter.id(), "arbiter_window_ns": arbiter_window_ns.to_string(),
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
    let verifier = deploy_init(
        &root,
        "verifier",
        wasm(&WASM_PP, "./").await?,
        json!({ "escrow": arras.id() }),
    )
    .await?;
    let now = w.view_block().await?.timestamp();
    Ok(Env {
        w,
        root,
        ft,
        arras,
        capturer,
        arbiter,
        payer,
        worker,
        tercero,
        verifier,
        now,
    })
}

impl Env {
    /// authorize real (payer -> worker) por `ft_transfer_call`, con `msg` `authorize` de v1 (sin `dispute_deadline`:
    /// el escrow toma `release_deadline`); devuelve el `payment_id`.
    async fn authorize(&self, amount: u128, salt: u8) -> anyhow::Result<String> {
        let msg = json!({
            "v": 1,
            "action": "authorize",
            "operator_id": "em",
            "receiver": self.worker.id(),
            "release_deadline": (self.now + DAY_NS).to_string(),
            "reclaim_after": (self.now + 2 * DAY_NS).to_string(),
            "fee_bps_max": 1300,
            "terms_hash": "0".repeat(64),
            "salt": salt_hex(salt),
        })
        .to_string();
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

    async fn arras_ok(&self, from: &Account, method: &str, args: Value) -> anyhow::Result<()> {
        let _ = call_ok(from, self.arras.id(), method, args, ZERO).await?;
        Ok(())
    }

    async fn payment(&self, id: &str) -> anyhow::Result<Value> {
        Ok(self
            .root
            .view(self.arras.id(), "get_payment")
            .args_json(json!({ "payment_id": id }))
            .await?
            .json()?)
    }

    /// `verify_feedback_proof` en `verifier`, firmado por un tercero (cualquiera puede llamarlo).
    async fn verify_en(
        &self,
        verifier: &AccountId,
        client: &AccountId,
        proof: &str,
        gas: Gas,
    ) -> anyhow::Result<ExecutionFinalResult> {
        Ok(self
            .tercero
            .call(verifier, "verify_feedback_proof")
            .args_json(json!({ "client": client, "proof": proof }))
            .gas(gas)
            .transact()
            .await?)
    }

    async fn verify_ok_en(
        &self,
        verifier: &AccountId,
        client: &AccountId,
        proof: &str,
    ) -> anyhow::Result<Value> {
        let r = self
            .verify_en(verifier, client, proof, Gas::from_tgas(50))
            .await?;
        assert!(r.is_success(), "verify: {:?}", r.clone().into_result());
        Ok(r.json()?)
    }

    async fn verify_falla_en(
        &self,
        verifier: &AccountId,
        client: &AccountId,
        proof: &str,
        code: &str,
    ) -> anyhow::Result<()> {
        let r = self
            .verify_en(verifier, client, proof, Gas::from_tgas(50))
            .await?;
        let p = panic_de(r);
        assert!(p.contains(code), "esperaba {code}: {p}");
        Ok(())
    }

    async fn verify_ok(&self, client: &AccountId, proof: &str) -> anyhow::Result<Value> {
        self.verify_ok_en(self.verifier.id(), client, proof).await
    }

    async fn verify_falla(
        &self,
        client: &AccountId,
        proof: &str,
        code: &str,
    ) -> anyhow::Result<()> {
        self.verify_falla_en(self.verifier.id(), client, proof, code)
            .await
    }

    /// Lo que tiene que devolver para cada parte un pago con `weight` liberado.
    async fn las_dos_partes(&self, id: &str, weight: u128) -> anyhow::Result<()> {
        assert_eq!(
            self.verify_ok(self.payer.id(), id).await?,
            json!({ "counterparty": self.worker.id(), "role": "receiver", "context": "em", "weight": weight.to_string() })
        );
        assert_eq!(
            self.verify_ok(self.worker.id(), id).await?,
            json!({ "counterparty": self.payer.id(), "role": "payer", "context": "em", "weight": weight.to_string() })
        );
        self.verify_falla(self.tercero.id(), id, "ERC8004_ARV_E15")
            .await
    }

    /// Las dos partes reciben `E13` (y el tercero tambien: `released == 0` se mira antes que la parte).
    async fn sin_liberar(&self, id: &str) -> anyhow::Result<()> {
        for who in [self.payer.id(), self.worker.id(), self.tercero.id()] {
            self.verify_falla(who, id, "ERC8004_ARV_E13").await?;
        }
        Ok(())
    }

    /// La disputa vencida a favor del receiver: `E16` para las dos partes y el tercero (se mira antes que la parte).
    async fn vencida_al_receiver(&self, id: &str) -> anyhow::Result<()> {
        for who in [self.payer.id(), self.worker.id(), self.tercero.id()] {
            self.verify_falla(who, id, "ERC8004_ARV_E16").await?;
        }
        Ok(())
    }

    async fn pasar_el_plazo(&self, hasta: u64) -> anyhow::Result<()> {
        let mut saltos = 0;
        while self.w.view_block().await?.timestamp() <= hasta {
            self.w.fast_forward(100).await?;
            saltos += 1;
            assert!(saltos < 100, "el reloj del sandbox no avanza");
        }
        Ok(())
    }
}

/// WP-01: el ciclo sin disputa.
#[tokio::test]
async fn wp_01_liberado_parcial_refund_y_client_ajeno() -> anyhow::Result<()> {
    let _permiso = SANDBOXES.acquire().await?;
    let e = setup(3 * DAY_NS).await?;
    // Liberado entero.
    let a = e.authorize(USDC, 1).await?;
    e.sin_liberar(&a).await?;
    e.arras_ok(
        &e.capturer,
        "release",
        json!({ "payment_id": a, "amount": null }),
    )
    .await?;
    assert_eq!(e.payment(&a).await?["status"], "Released");
    e.las_dos_partes(&a, USDC).await?;
    // Parcial: 400.000; despues el refund del resto. Lo liberado sigue valiendo.
    let b = e.authorize(USDC, 2).await?;
    e.arras_ok(
        &e.capturer,
        "release",
        json!({ "payment_id": b, "amount": "400000" }),
    )
    .await?;
    assert_eq!(e.payment(&b).await?["status"], "PartiallyReleased");
    e.las_dos_partes(&b, 400_000).await?;
    e.arras_ok(&e.capturer, "refund", json!({ "payment_id": b }))
        .await?;
    let pb = e.payment(&b).await?;
    assert_eq!(
        (&pb["status"], &pb["released"], &pb["refunded"]),
        (&json!("Refunded"), &json!("400000"), &json!("600000"))
    );
    e.las_dos_partes(&b, 400_000).await?;
    // Reembolsado sin liberar nada: released 0.
    let c = e.authorize(USDC, 3).await?;
    e.arras_ok(&e.worker, "refund", json!({ "payment_id": c }))
        .await?;
    assert_eq!(e.payment(&c).await?["status"], "Refunded");
    e.sin_liberar(&c).await?;
    Ok(())
}

/// WP-02: la disputa resuelta por el arbitro, con y sin liberacion previa.
#[tokio::test]
async fn wp_02_disputa_abierta_y_resuelta_por_el_arbitro() -> anyhow::Result<()> {
    let _permiso = SANDBOXES.acquire().await?;
    let e = setup(3 * DAY_NS).await?;
    // 300.000 liberados y despues el receiver abre una disputa: lo liberado vale mientras esta abierta.
    let a = e.authorize(USDC, 1).await?;
    e.arras_ok(
        &e.capturer,
        "release",
        json!({ "payment_id": a, "amount": "300000" }),
    )
    .await?;
    e.arras_ok(
        &e.worker,
        "open_dispute",
        json!({ "payment_id": a, "reason_hash": RAZON }),
    )
    .await?;
    let pa = e.payment(&a).await?;
    assert_eq!(pa["status"], "Disputed");
    assert_eq!(pa["dispute"]["opened_by"], "Receiver");
    assert_eq!(pa["dispute"]["resolved_at"], Value::Null);
    e.las_dos_partes(&a, 300_000).await?;
    // El arbitro le da 200.000 mas al receiver y 500.000 al payer.
    e.arras_ok(
        &e.arbiter,
        "resolve_dispute",
        json!({ "payment_id": a, "to_receiver": "200000", "to_payer": "500000" }),
    )
    .await?;
    let pa = e.payment(&a).await?;
    assert_eq!(
        (&pa["status"], &pa["released"], &pa["refunded"]),
        (&json!("Resolved"), &json!("500000"), &json!("500000"))
    );
    assert!(pa["dispute"]["resolved_at"].is_string());
    e.las_dos_partes(&a, 500_000).await?;
    // El payer abre sin liberacion previa: E13 abierta, y E13 si el arbitro le devuelve todo.
    let b = e.authorize(USDC, 2).await?;
    e.arras_ok(
        &e.payer,
        "open_dispute",
        json!({ "payment_id": b, "reason_hash": RAZON }),
    )
    .await?;
    assert_eq!(e.payment(&b).await?["status"], "Disputed");
    e.sin_liberar(&b).await?;
    e.arras_ok(
        &e.arbiter,
        "resolve_dispute",
        json!({ "payment_id": b, "to_receiver": "0", "to_payer": USDC.to_string() }),
    )
    .await?;
    let pb = e.payment(&b).await?;
    assert_eq!(
        (&pb["status"], &pb["released"]),
        (&json!("Resolved"), &json!("0"))
    );
    e.sin_liberar(&b).await?;
    Ok(())
}

/// WP-03: las disputas vencidas. La abre el payer antes de `release_deadline`: lo vivo va al receiver sin capturer ni
/// arbitro, y no es prueba (`E16`), tampoco por lo que el capturer libero antes. La abre el receiver: lo vivo vuelve al
/// payer, y vale lo liberado antes (o `E13`).
#[tokio::test]
async fn wp_03_disputas_vencidas() -> anyhow::Result<()> {
    let _permiso = SANDBOXES.acquire().await?;
    let e = setup(MIN_WINDOW_NS).await?;
    let a = e.authorize(USDC, 1).await?;
    let b = e.authorize(USDC, 2).await?;
    let c = e.authorize(USDC, 3).await?;
    let d = e.authorize(USDC, 4).await?;
    for id in [&c, &d] {
        e.arras_ok(
            &e.capturer,
            "release",
            json!({ "payment_id": id, "amount": "300000" }),
        )
        .await?;
    }
    for (id, quien) in [
        (&a, &e.payer),
        (&b, &e.worker),
        (&c, &e.payer),
        (&d, &e.worker),
    ] {
        e.arras_ok(
            quien,
            "open_dispute",
            json!({ "payment_id": id, "reason_hash": RAZON }),
        )
        .await?;
    }
    e.sin_liberar(&a).await?;
    e.sin_liberar(&b).await?;
    // Abiertas, lo liberado antes vale (lo libero el capturer), la abra quien la abra.
    e.las_dos_partes(&c, 300_000).await?;
    e.las_dos_partes(&d, 300_000).await?;
    let opened: u64 = e.payment(&d).await?["dispute"]["opened_at"]
        .as_str()
        .unwrap()
        .parse()?;
    e.pasar_el_plazo(opened + MIN_WINDOW_NS).await?;
    for id in [&a, &b, &c, &d] {
        e.arras_ok(
            &e.tercero,
            "resolve_expired_dispute",
            json!({ "payment_id": id }),
        )
        .await?;
    }
    // El `Payment` que lee el verificador: la vencida (`resolved_at >= opened_at + arbiter_window_ns`) del payer antes
    // de `release_deadline`.
    for id in [&a, &c] {
        let p = e.payment(id).await?;
        assert_eq!(
            (
                &p["status"],
                &p["released"],
                &p["refunded"],
                &p["dispute"]["opened_by"]
            ),
            (
                &json!("Resolved"),
                &json!("1000000"),
                &json!("0"),
                &json!("Payer")
            )
        );
        let n = |v: &Value| v.as_str().unwrap().parse::<u64>().unwrap();
        assert!(
            n(&p["dispute"]["resolved_at"])
                >= n(&p["dispute"]["opened_at"]) + n(&p["arbiter_window_ns"])
        );
        assert!(n(&p["dispute"]["opened_at"]) < n(&p["release_deadline"]));
        e.vencida_al_receiver(id).await?;
    }
    let pb = e.payment(&b).await?;
    assert_eq!(
        (&pb["status"], &pb["released"], &pb["refunded"]),
        (&json!("Resolved"), &json!("0"), &json!("1000000"))
    );
    e.sin_liberar(&b).await?;
    let pd = e.payment(&d).await?;
    assert_eq!(
        (&pd["status"], &pd["released"], &pd["refunded"]),
        (&json!("Resolved"), &json!("300000"), &json!("700000"))
    );
    e.las_dos_partes(&d, 300_000).await?;
    Ok(())
}

/// WP-04: lo que falla antes o fuera del pago.
#[tokio::test]
async fn wp_04_inexistente_no_canonica_gas_y_escrow_sin_codigo() -> anyhow::Result<()> {
    let _permiso = SANDBOXES.acquire().await?;
    let e = setup(3 * DAY_NS).await?;
    e.verify_falla(e.payer.id(), PID_NADIE, "ERC8004_ARV_E12")
        .await?;
    for bad in [
        format!("1{PID_NADIE}"),
        PID_NADIE.to_lowercase(),
        String::new(),
        "panic".to_string(),
    ] {
        e.verify_falla(e.payer.id(), &bad, "ERC8004_ARV_E02")
            .await?;
    }
    let r = e
        .verify_en(e.verifier.id(), e.payer.id(), PID_NADIE, Gas::from_tgas(24))
        .await?;
    assert!(panic_de(r).contains("ERC8004_ARV_E03"));
    // Un verificador apuntado a una cuenta sin codigo: get_payment falla -> E10.
    let vacia = sub(&e.root, "vacia", 1).await?;
    let otro = deploy_init(
        &e.root,
        "verifier-vacio",
        wasm(&WASM_PP, "./").await?,
        json!({ "escrow": vacia.id() }),
    )
    .await?;
    e.verify_falla_en(otro.id(), e.payer.id(), PID_NADIE, "ERC8004_ARV_E10")
        .await?;
    // Con el gas minimo justo (25 TGas, en la VM de verdad) llega al callback y verifica.
    let id = e.authorize(USDC, 1).await?;
    e.arras_ok(
        &e.capturer,
        "release",
        json!({ "payment_id": id, "amount": null }),
    )
    .await?;
    let r = e
        .verify_en(e.verifier.id(), e.payer.id(), &id, Gas::from_tgas(25))
        .await?;
    assert!(r.is_success(), "25 TGas: {:?}", r.clone().into_result());
    assert_eq!(r.json::<Value>()?["weight"], json!(USDC.to_string()));
    // `on_payment` es privado: un tercero no puede llamarlo con una respuesta a medida.
    let r = e
        .tercero
        .call(e.verifier.id(), "on_payment")
        .args_json(json!({ "client": e.payer.id() }))
        .gas(Gas::from_tgas(30))
        .transact()
        .await?;
    assert!(panic_de(r).contains("is private"));
    // `new` es privado (en una cuenta con el codigo y sin estado, un tercero no la inicializa), no se puede llamar
    // dos veces, y el escrow no puede ser la propia cuenta.
    let sin_init = sub(&e.root, "sin-init", 10).await?;
    assert!(
        sin_init
            .deploy(wasm(&WASM_PP, "./").await?)
            .await?
            .is_success()
    );
    let r = e
        .tercero
        .call(sin_init.id(), "new")
        .args_json(json!({ "escrow": e.arras.id() }))
        .gas(Gas::from_tgas(30))
        .transact()
        .await?;
    assert!(panic_de(r).contains("is private"));
    let r = e
        .tercero
        .call(e.verifier.id(), "new")
        .args_json(json!({ "escrow": e.tercero.id() }))
        .gas(Gas::from_tgas(30))
        .transact()
        .await?;
    assert!(r.is_failure());
    let r = e
        .verifier
        .call(e.verifier.id(), "new")
        .args_json(json!({ "escrow": e.tercero.id() }))
        .gas(Gas::from_tgas(30))
        .transact()
        .await?;
    assert!(panic_de(r).contains("already been initialized"));
    let propio = sub(&e.root, "propio", 10).await?;
    let r = propio
        .batch(propio.id())
        .deploy(wasm(&WASM_PP, "./").await?)
        .call(
            Function::new("new")
                .args_json(json!({ "escrow": propio.id() }))
                .gas(Gas::from_tgas(30)),
        )
        .transact()
        .await?;
    assert!(panic_de(r).contains("ERC8004_ARV_E01"));
    let cfg: Value = e.root.view(e.verifier.id(), "get_config").await?.json()?;
    assert_eq!(cfg["escrow"], json!(e.arras.id()));
    assert_eq!(cfg["contract"], "erc8004-arras-verifier");
    Ok(())
}

/// WP-05: el escrow de mentira devuelve los bytes que se le cargan. El `Payment` real de un pago liberado, copiado tal
/// cual, vale; cada cambio que el escrow real no hace, no.
#[tokio::test]
async fn wp_05_escrow_de_mentira() -> anyhow::Result<()> {
    let _permiso = SANDBOXES.acquire().await?;
    let e = setup(3 * DAY_NS).await?;
    let id = e.authorize(USDC, 1).await?;
    e.arras_ok(
        &e.capturer,
        "release",
        json!({ "payment_id": id, "amount": "700000" }),
    )
    .await?;
    let crudo = e
        .root
        .view(e.arras.id(), "get_payment")
        .args_json(json!({ "payment_id": id }))
        .await?
        .result;
    let real: Value = serde_json::from_slice(&crudo)?;
    assert_eq!(real.as_object().unwrap().len(), 19);
    let mentira = deploy_init(
        &e.root,
        "mentira",
        wasm(&WASM_TE, "../test-escrow").await?,
        json!({}),
    )
    .await?;
    let ver = deploy_init(
        &e.root,
        "verifier-mentira",
        wasm(&WASM_PP, "./").await?,
        json!({ "escrow": mentira.id() }),
    )
    .await?;
    let cargar = |raw: String| {
        let (mentira, id) = (mentira.clone(), id.clone());
        async move {
            call_ok(
                &mentira,
                mentira.id(),
                "set_raw",
                json!({ "payment_id": id, "raw": raw }),
                ZERO,
            )
            .await
        }
    };
    // Los bytes del escrow real, tal cual: lo mismo que el verificador del escrow real.
    let _ = cargar(String::from_utf8(crudo.clone())?).await?;
    assert_eq!(
        e.verify_ok_en(ver.id(), e.payer.id(), &id).await?,
        e.verify_ok(e.payer.id(), &id).await?
    );
    let mut casos: Vec<(&str, String, &str)> = vec![];
    let mut de_mas = real.clone();
    de_mas["budget"] = json!(null);
    casos.push(("un campo de mas", de_mas.to_string(), "ERC8004_ARV_E11"));
    let mut de_mas_disputa = real.clone();
    de_mas_disputa["status"] = json!("Disputed");
    de_mas_disputa["dispute"] = json!({
        "opened_by": "Receiver", "reason_hash": RAZON, "opened_at": "1", "resolved_at": null, "arbiter": "x.near"
    });
    casos.push((
        "un campo de mas en la disputa",
        de_mas_disputa.to_string(),
        "ERC8004_ARV_E11",
    ));
    let mut v1 = real.clone();
    for campo in ["dispute_deadline", "arbiter_window_ns", "dispute"] {
        v1.as_object_mut().unwrap().remove(campo);
    }
    casos.push(("el Payment de v1", v1.to_string(), "ERC8004_ARV_E11"));
    let mut incoherente = real.clone();
    incoherente["status"] = json!("Resolved");
    casos.push((
        "Resolved sin dispute",
        incoherente.to_string(),
        "ERC8004_ARV_E11",
    ));
    let mut auto = real.clone();
    auto["receiver"] = json!(e.payer.id());
    casos.push(("payer == receiver", auto.to_string(), "ERC8004_ARV_E14"));
    let mut largo = real.to_string();
    largo.push_str(&" ".repeat(8 * 1024));
    casos.push(("mas de 8 KiB", largo, "ERC8004_ARV_E11"));
    casos.push(("JSON roto", "{".into(), "ERC8004_ARV_E11"));
    casos.push(("null", "null".into(), "ERC8004_ARV_E12"));
    for (que, raw, code) in casos {
        let _ = cargar(raw).await?;
        for who in [e.payer.id(), e.worker.id()] {
            let r = e.verify_en(ver.id(), who, &id, Gas::from_tgas(50)).await?;
            let p = panic_de(r);
            assert!(p.contains(code), "{que}: esperaba {code}: {p}");
        }
    }
    Ok(())
}

/// WP-06: el ciclo entero con el Reputation Registry (el mismo crate que `reputation-registry.erc-8004.near`, sin
/// cambios): el payer califica al agente del worker con la prueba del pago del escrow.
#[tokio::test]
async fn wp_06_ciclo_con_el_reputation_registry() -> anyhow::Result<()> {
    let _permiso = SANDBOXES.acquire().await?;
    let e = setup(3 * DAY_NS).await?;
    let identity = deploy_init(
        &e.root,
        "identity",
        wasm(&WASM_ID, "../erc8004-identity").await?,
        json!({}),
    )
    .await?;
    let reputation = deploy_init(
        &e.root,
        "reputation",
        wasm(&WASM_REP, "../erc8004-reputation").await?,
        json!({ "identity_registry": identity.id() }),
    )
    .await?;
    let icfg: Value = e.root.view(identity.id(), "get_config").await?.json()?;
    let rcfg: Value = e.root.view(reputation.id(), "get_config").await?.json()?;
    let register_deposit: u128 = icfg["register_deposit"].as_str().unwrap().parse()?;
    let proof_deposit: u128 = rcfg["feedback_proof_deposit"].as_str().unwrap().parse()?;
    let r = e
        .worker
        .call(identity.id(), "register")
        .args_json(json!({ "agent_uri": "https://agente.example/agent.json" }))
        .deposit(NearToken::from_yoctonear(register_deposit))
        .gas(Gas::from_tgas(30))
        .transact()
        .await?;
    assert!(r.is_success(), "register: {:?}", r.clone().into_result());
    let agent: String = r.json()?;
    // Un pago con disputa: 600.000 liberados, la disputa del payer y el arbitro que le da 100.000 mas al receiver.
    let id = e.authorize(USDC, 1).await?;
    e.arras_ok(
        &e.capturer,
        "release",
        json!({ "payment_id": id, "amount": "600000" }),
    )
    .await?;
    e.arras_ok(
        &e.payer,
        "open_dispute",
        json!({ "payment_id": id, "reason_hash": RAZON }),
    )
    .await?;
    e.arras_ok(
        &e.arbiter,
        "resolve_dispute",
        json!({ "payment_id": id, "to_receiver": "100000", "to_payer": "300000" }),
    )
    .await?;
    let give = || {
        e.payer
            .call(reputation.id(), "give_feedback_with_proof")
            .args_json(json!({
                "agent_id": agent, "value": "40", "value_decimals": 0, "tag1": "starred", "tag2": "em",
                "verifier": e.verifier.id(), "proof": id,
            }))
            .deposit(NearToken::from_yoctonear(proof_deposit))
            .gas(Gas::from_tgas(150))
            .transact()
    };
    let r = give().await?;
    assert!(r.is_success(), "{:?}", r.clone().into_result());
    let index: Value = r.clone().json()?;
    assert_eq!(index, json!("1"), "logs: {:?}", r.logs());
    let total = r.total_gas_burnt;
    let prueba: Value = e
        .root
        .view(reputation.id(), "read_feedback_proof")
        .args_json(
            json!({ "agent_id": agent, "client_address": e.payer.id(), "feedback_index": "1" }),
        )
        .await?
        .json()?;
    assert_eq!(
        prueba,
        json!({
            "verifier": e.verifier.id(), "proof": id, "counterparty": e.worker.id(),
            "role": "receiver", "context": "em", "weight": "700000",
        })
    );
    let resumen: Value = e
        .root
        .view(reputation.id(), "get_verified_summary")
        .args_json(json!({ "agent_id": agent, "verifier": e.verifier.id(), "context": "em" }))
        .await?
        .json()?;
    assert_eq!(
        (&resumen["count"], &resumen["weight_sum"]),
        (&json!("1"), &json!("700000"))
    );
    // La misma prueba otra vez: el registro la rechaza antes de la llamada cruzada (E42).
    let err = panic_de(give().await?);
    assert!(err.contains("ERC8004_REP_E42"), "{err}");
    println!(
        "MEDIDA WP-06 give_feedback_with_proof aceptada (pago Resolved): {:.3} TGas total",
        total.as_gas() as f64 / 1e12
    );
    Ok(())
}

/// WP-07: gas de `verify_feedback_proof`, storage del verificador desplegado (con su llave y sin ella) y que sin
/// llaves sigue andando.
#[tokio::test]
async fn wp_07_medidas_y_sin_llaves() -> anyhow::Result<()> {
    let _permiso = SANDBOXES.acquire().await?;
    let e = setup(3 * DAY_NS).await?;
    let id = e.authorize(USDC, 1).await?;
    e.arras_ok(
        &e.capturer,
        "release",
        json!({ "payment_id": id, "amount": null }),
    )
    .await?;
    let r = e
        .verify_en(e.verifier.id(), e.payer.id(), &id, Gas::from_tgas(50))
        .await?;
    assert!(r.is_success());
    let por_recibo: Vec<String> = r
        .receipt_outcomes()
        .iter()
        .map(|o| {
            format!(
                "{}: {:.3}",
                o.executor_id,
                o.gas_burnt.as_gas() as f64 / 1e12
            )
        })
        .collect();
    println!(
        "MEDIDA WP-07 verify_feedback_proof: {:.3} TGas total; recibos {por_recibo:?}",
        r.total_gas_burnt.as_gas() as f64 / 1e12
    );
    let wasm_len = wasm(&WASM_PP, "./").await?.len();
    let con_llave = e.w.view_account(e.verifier.id()).await?.storage_usage;
    let pk = e.verifier.secret_key().public_key();
    let r = e
        .verifier
        .batch(e.verifier.id())
        .delete_key(pk)
        .transact()
        .await?;
    assert!(r.is_success(), "{:?}", r.into_result());
    let sin_llave = e.w.view_account(e.verifier.id()).await?.storage_usage;
    let llaves = e.w.view_access_keys(e.verifier.id()).await?;
    assert!(llaves.is_empty());
    println!(
        "MEDIDA WP-07 verificador: wasm (compile_project) {wasm_len} bytes; storage con su llave {con_llave}, sin llaves {sin_llave} (cuenta + estado = {})",
        sin_llave - wasm_len as u64
    );
    e.las_dos_partes(&id, USDC).await?;
    Ok(())
}
