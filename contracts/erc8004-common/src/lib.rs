//! Lo comun del port de ERC-8004 a NEAR (`docs/plans/erc8004-near-port.md`). No tiene estado de contrato ni metodos
//! exportados: lo usan `erc8004-identity`, `erc8004-reputation`, `erc8004-arras-verifier` y `erc8004-validation`.
//!
//! - [`validate`]: las validaciones de entrada (URIs de RFC 3986, claves, tags, hashes, enteros canonicos, `value`).
//! - [`summary`]: el promedio de `getSummary` de EVM, en 256 bits como el `int256` de Solidity.
//! - [`types`]: los esquemas JSON de las llamadas cruzadas (`check_agent` de Identity y `verify_feedback_proof` de un
//!   verificador), estrictos al deserializar.
//! - [`events`]: el sobre NEP-297.
//! - [`storage`]: el costo del byte y las devoluciones.

pub mod events;
pub mod storage;
pub mod summary;
pub mod types;
pub mod validate;

/// La version de la implementacion de referencia de ERC-8004 que porta este codigo (`getVersion()` de los tres
/// contratos oficiales en `erc-8004/erc-8004-contracts` b9e466c, la que responde Base mainnet).
pub const ERC8004_VERSION: &str = "2.0.0";

/// Largo maximo de un `AccountId` de NEAR.
pub const MAX_ACCOUNT_ID_LEN: usize = 64;
