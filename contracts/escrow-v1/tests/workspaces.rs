//! Tests contra la wasm real en el sandbox de near-workspaces (sec. 5). U-24 vive aca porque `#[private]` solo
//! existe en el wrapper de la wasm (`near-sdk-macros`, `#[cfg(target_arch = "wasm32")]`): un test unitario que
//! llama `Arras::new` directo no lo ve.

use serde_json::json;
use tokio::sync::OnceCell;

static WASM_V1: OnceCell<Vec<u8>> = OnceCell::const_new();

/// La wasm de v1, compilada una vez por corrida con cargo-near-build (`compile_project`).
async fn wasm_v1() -> anyhow::Result<&'static [u8]> {
    let w = WASM_V1
        .get_or_try_init(|| async { near_workspaces::compile_project("./").await })
        .await?;
    Ok(w.as_slice())
}

/// U-24 `new_ajeno`: `new` llamado por otra cuenta -> panic (`#[private]`); la propia cuenta si inicializa.
#[tokio::test]
async fn u24_new_ajeno() -> anyhow::Result<()> {
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
    let cfg: serde_json::Value = contract.view("get_config").await?.json()?;
    assert_eq!(cfg["version"], "arras-v1");
    // Una segunda inicializacion falla (estado ya existe).
    let again = contract.call("new").args_json(&args).transact().await?;
    assert!(again.is_failure());
    Ok(())
}
