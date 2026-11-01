//! Arras v1: escrow sin custodia de un solo token (USDC nativo de NEAR), inmutable, sin `migrate` y sin llaves
//! despues del deploy. Especificacion: prescripcion del escrow Arras, sec. 3.
//!
//! Modelo pull: `release`, `refund` y `reclaim` no hacen promesas, solo acreditan `claimable`; `withdraw` y
//! `withdraw_for` ponen `claimable` en 0 (sin borrar la entrada) y `on_withdraw(Err)` la suma de vuelta (`+=`).
//!
//! TVL (sec. 3.5): `tvl_global` sube en authorize y baja **solo** en `on_withdraw` Ok; `tvl_operador` y
//! `payer_tvl` bajan con lo que sale de "vivo". Invariante: `sum(vivos) + sum(claimable) + in_flight == tvl_global
//! <= ft_balance_of(contrato)`.

use arras_escrow_core::ArrasError;
use near_sdk::json_types::{U64, U128};
use near_sdk::store::{IterableMap, LookupMap, LookupSet, Vector};
use near_sdk::{AccountId, Gas, NearToken, PanicOnDefault, env, near};

mod admin;
mod storage;
mod views;

pub use arras_escrow_core::{MAX_FEE_BPS, PaymentId};
pub use views::{ConfigView, StorageHealth};

#[cfg(test)]
mod unit_tests;

/// `get_config.version`.
pub const VERSION: &str = "arras-v1";
/// Gas del `ft_transfer` de cada retiro [S-1].
pub const GAS_FT_TRANSFER: Gas = Gas::from_tgas(10);
/// Gas reservado para `on_withdraw` [S-1].
pub const GAS_ON_WITHDRAW: Gas = Gas::from_tgas(10);
/// Retencion antes de `compact`: 7 dias despues de `reclaim_after`.
pub const RETENTION_NS: u64 = 7 * 24 * 60 * 60 * 1_000_000_000;
/// `get_storage_health` alarma si la reserva libre baja de 0,2 NEAR.
pub const MIN_FREE_RESERVE: NearToken = NearToken::from_millinear(200);
/// Tope de `limit` en `list_payments`.
pub const MAX_LIST_LIMIT: u32 = 50;
/// Bytes que el protocolo suma por cada registro de storage (`num_extra_bytes_record`).
pub const RECORD_OVERHEAD_BYTES: u64 = 40;
/// Largo maximo de un `AccountId` de NEAR.
pub const MAX_ACCOUNT_ID_LEN: u64 = 64;

/// `operator_id`: `[a-z0-9_-]{1,32}`.
pub type OperatorId = String;

/// Prefijos de storage de un byte (sec. 3.1).
pub mod prefix {
    pub const OPERATORS: &[u8] = b"o";
    pub const PAYMENTS: &[u8] = b"p";
    pub const PAYMENTS_BY_OPERATOR: &[u8] = b"i";
    pub const USED_IDS: &[u8] = b"u";
    pub const CLAIMABLE: &[u8] = b"c";
    pub const PAYER_TVL: &[u8] = b"v";
    pub const STORAGE: &[u8] = b"s";
    pub const SPONSORED_USED: &[u8] = b"z";
}

/// Estado de un pago (sec. 7, maquina de estados). v2 suma `Disputed` y `Resolved` al final.
#[near(serializers = [borsh, json])]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaymentStatus {
    Authorized,
    PartiallyReleased,
    Released,
    Refunded,
    Reclaimed,
}

impl PaymentStatus {
    /// `Authorized` o `PartiallyReleased`: todavia tiene plata "viva".
    pub fn is_live(&self) -> bool {
        matches!(self, Self::Authorized | Self::PartiallyReleased)
    }
}

/// Valor de `payments` (sec. 3.1).
#[near(serializers = [borsh, json])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Payment {
    pub operator_id: OperatorId,
    /// `sender_id` de `ft_on_transfer`, nunca el predecessor (que es el token).
    pub payer: AccountId,
    pub receiver: AccountId,
    pub amount: U128,
    pub released: U128,
    pub refunded: U128,
    /// Congelada al autorizar = `operator.fee_bps`.
    pub fee_bps: u16,
    /// Congelado al autorizar: el owner no lo redirige para pagos vivos.
    pub fee_recipient: AccountId,
    /// Firmado por el payer.
    pub fee_bps_max: u16,
    pub created_at: U64,
    pub release_deadline: U64,
    pub reclaim_after: U64,
    /// Hex de 64 en minusculas.
    pub terms_hash: String,
    pub status: PaymentStatus,
    /// Yocto cobrados al payer (NEP-145) o al patrocinio; vuelven solo al compactar.
    pub storage_charged: U128,
    /// Si lo pago el `sponsor_pool` del operador.
    pub sponsored: bool,
}

impl Payment {
    /// `amount - released - refunded`: lo que sigue "vivo".
    pub fn live_amount(&self) -> u128 {
        self.amount
            .0
            .checked_sub(self.released.0)
            .and_then(|r| r.checked_sub(self.refunded.0))
            .unwrap_or_else(|| env::abort())
    }
}

/// Valor de `operators` (sec. 3.1).
#[near(serializers = [borsh, json])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Operator {
    pub owner: AccountId,
    pub capturers: Vec<AccountId>,
    /// `<= 1800` (`FASE5_MAX_FEE_BPS` de EM).
    pub fee_bps: u16,
    pub fee_recipient: AccountId,
    pub min_amount: U128,
    pub tope_tvl_operador: U128,
    pub tvl_operador: U128,
    pub tope_tvl_por_payer: U128,
    /// Yocto para patrocinar storage de payers sin NEP-145.
    pub sponsor_pool: U128,
    /// Yocto de patrocinio por payer.
    pub sponsor_max_por_payer: U128,
}

/// Saldo NEP-145 de una cuenta: el mismo layout borsh que `StorageBalance` (dos u128) con `Clone` y `Debug`.
#[near(serializers = [borsh])]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StorageAccount {
    pub total: u128,
    pub available: u128,
}

/// `register_operator`: los campos de `Operator` sin `tvl_operador` ni `sponsor_pool` (el pool solo se fondea con
/// NEAR real por `fund_operator_storage`; ver PREGUNTA-c0der.md).
#[near(serializers = [json])]
#[serde(deny_unknown_fields)]
#[derive(Clone, Debug)]
pub struct OperatorInput {
    pub owner: AccountId,
    pub capturers: Vec<AccountId>,
    pub fee_bps: u16,
    pub fee_recipient: AccountId,
    pub min_amount: U128,
    pub tope_tvl_operador: U128,
    pub tope_tvl_por_payer: U128,
    pub sponsor_max_por_payer: U128,
}

/// `update_operator`: los editables, en `Option`. Los topes solo bajan (`E42`).
#[near(serializers = [json])]
#[serde(deny_unknown_fields)]
#[derive(Clone, Debug, Default)]
pub struct OperatorPatch {
    pub capturers: Option<Vec<AccountId>>,
    pub fee_bps: Option<u16>,
    pub fee_recipient: Option<AccountId>,
    pub min_amount: Option<U128>,
    pub tope_tvl_operador: Option<U128>,
    pub tope_tvl_por_payer: Option<U128>,
    pub sponsor_max_por_payer: Option<U128>,
}

/// Contrato `Arras` (sec. 3.1).
#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Arras {
    owner: AccountId,
    guardian: AccountId,
    paused: bool,
    /// Unico y fijo desde `new`.
    token: AccountId,
    tope_por_pago: u128,
    tope_tvl_global: u128,
    tvl_global: u128,
    /// Suma de retiros en vuelo.
    in_flight: u128,
    /// Suma de los `available` de NEP-145: plata de los payers, no reserva del contrato (`get_storage_health`).
    storage_available_total: u128,
    operators: IterableMap<OperatorId, Operator>,
    payments: IterableMap<PaymentId, Payment>,
    /// Indice para `list_payments`; los ids compactados quedan en el indice y la vista los salta.
    payments_by_operator: LookupMap<OperatorId, Vector<PaymentId>>,
    /// Lapida de ids compactados (el hash de 32 bytes del `payment_id`).
    used_ids: LookupSet<[u8; 32]>,
    /// Una entrada por cuenta; **nunca se borra** (`withdraw` la pone en 0).
    claimable: LookupMap<AccountId, u128>,
    payer_tvl: LookupMap<(OperatorId, AccountId), u128>,
    /// NEP-145.
    storage: LookupMap<AccountId, StorageAccount>,
    sponsored_used: LookupMap<(OperatorId, AccountId), u128>,
}

/// Prefijo del `Vector` de ids de un operador: `i` + borsh(operator_id) + `/`.
pub(crate) fn index_prefix(operator_id: &str) -> Vec<u8> {
    let mut p = prefix::PAYMENTS_BY_OPERATOR.to_vec();
    p.extend_from_slice(&(operator_id.len() as u32).to_le_bytes());
    p.extend_from_slice(operator_id.as_bytes());
    p.push(b'/');
    p
}

/// Bytes de un registro de storage: overhead + clave + valor.
pub(crate) const fn record_bytes(key_len: u64, value_len: u64) -> u64 {
    RECORD_OVERHEAD_BYTES + key_len + value_len
}

/// Costo en yocto de `bytes` de storage.
pub(crate) fn bytes_cost(bytes: u64) -> u128 {
    u128::from(bytes)
        .checked_mul(env::storage_byte_cost().as_yoctonear())
        .unwrap_or_else(|| env::abort())
}

#[near]
impl Arras {
    /// Solo la propia cuenta (con `with-init-call` en el deploy): `#[private]` cierra el `new` ajeno si el deploy y
    /// el init quedaran en dos transacciones (refutacion P2-3).
    #[init]
    #[private]
    pub fn new(
        owner: AccountId,
        guardian: AccountId,
        token: AccountId,
        tope_por_pago: U128,
        tope_tvl_global: U128,
    ) -> Self {
        Self {
            owner,
            guardian,
            paused: false,
            token,
            tope_por_pago: tope_por_pago.0,
            tope_tvl_global: tope_tvl_global.0,
            tvl_global: 0,
            in_flight: 0,
            storage_available_total: 0,
            operators: IterableMap::new(prefix::OPERATORS),
            payments: IterableMap::new(prefix::PAYMENTS),
            payments_by_operator: LookupMap::new(prefix::PAYMENTS_BY_OPERATOR),
            used_ids: LookupSet::new(prefix::USED_IDS),
            claimable: LookupMap::new(prefix::CLAIMABLE),
            payer_tvl: LookupMap::new(prefix::PAYER_TVL),
            storage: LookupMap::new(prefix::STORAGE),
            sponsored_used: LookupMap::new(prefix::SPONSORED_USED),
        }
    }
}

impl Arras {
    pub(crate) fn assert_owner(&self) {
        if env::predecessor_account_id() != self.owner {
            ArrasError::E40SoloOwner.panic();
        }
    }

    pub(crate) fn claimable_of(&self, account: &AccountId) -> u128 {
        self.claimable.get(account).copied().unwrap_or(0)
    }

    /// Persiste los caches de todas las colecciones (en la wasm lo hace el `Drop` al final de la llamada; los tests
    /// lo llaman para medir `env::storage_usage()`).
    #[cfg(test)]
    pub(crate) fn flush_all(&mut self) {
        let ops: Vec<OperatorId> = self.operators.keys().cloned().collect();
        for op in ops {
            if let Some(v) = self.payments_by_operator.get_mut(&op) {
                v.flush();
            }
        }
        self.operators.flush();
        self.payments.flush();
        self.payments_by_operator.flush();
        self.claimable.flush();
        self.payer_tvl.flush();
        self.storage.flush();
        self.sponsored_used.flush();
    }
}
