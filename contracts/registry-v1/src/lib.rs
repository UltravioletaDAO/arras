//! Arras registry v1: identidad y reputacion de agentes en NEAR, con la forma de ERC-8004 (Identity Registry +
//! Reputation Registry), inmutable, sin `migrate` y sin llaves despues del deploy.
//!
//! - **Identidad**: la identidad es la propia cuenta NEAR (no se acunan NFT ni ids). `register` da de alta a la
//!   cuenta que llama con su `agent_uri` (la ficha, como el `tokenURI` de ERC-8004) y una `metadata` opcional;
//!   `update` la reemplaza. Cada uno paga su storage con el deposito adjunto y recibe el sobrante.
//! - **Reputacion**: `give_feedback` solo vale sobre un trato real de Arras. La verificacion es EN CADENA, en el
//!   callback de una llamada cruzada a `escrow.get_payment(payment_id)`: el pago existe, libero plata (`released >
//!   0`, cualquiera sea su estado: tambien un `Refunded` o `Reclaimed` tras una liberacion parcial), quien califica es
//!   el payer o el receiver y la calificada es la otra parte. Una sola calificacion por `(escrow, payment_id, rater)`, con candado mientras la llamada vuela. El escrow
//!   tiene que estar en la lista fija de `new` (contratos, no personas).
//!
//! El callback nunca entra en panico: un rechazo devuelve el deposito entero, deja el log
//! `arras-registry:rechazo:<CODIGO>` y libera el candado.

use arras_escrow_core::PaymentId;
use near_sdk::borsh;
use near_sdk::json_types::{U64, U128};
use near_sdk::store::{IterableMap, LookupMap, Vector};
use near_sdk::{AccountId, Gas, NearToken, PanicOnDefault, env, near};

mod error;
mod events;
mod identity;
mod reputation;
mod validate;
mod views;

pub use error::RegistryError;
pub use events::{
    AgentData, EVENT_STANDARD, EVENT_VERSION, FeedbackGivenData, FeedbackRevokedData,
    RegistryEvent, event_log,
};
pub use reputation::{EscrowPayment, PendingFeedback};
pub use validate::{hex, is_valid_tag, is_valid_uri, parse_hash};
pub use views::{AgentView, ConfigView, FeedbackView, Limits, RoleSummaryView, SummaryView};

#[cfg(test)]
mod unit_tests;

/// `get_config.version`.
pub const VERSION: &str = "arras-registry-v1";
/// Tope de escrows de la lista de `new`.
pub const MAX_ESCROWS: usize = 8;
pub const MAX_AGENT_URI_LEN: usize = 512;
pub const MAX_METADATA_LEN: usize = 1024;
pub const MAX_FEEDBACK_URI_LEN: usize = 512;
pub const MAX_TAG_LEN: usize = 64;
/// Un `payment_id` de Arras tiene de 32 a 44 caracteres (base58 de 32 bytes).
pub const MAX_PAYMENT_ID_LEN: usize = 64;
/// Bytes de `operator_id` que cubre `feedback_deposit`. v1 lo limita a 32 (`[a-z0-9_-]{1,32}`, que el callback exige:
/// fuera de eso es `E21`); el deposito se fijo con 64 y no se achico en la RONDA 1 para no cambiar la interfaz: ese
/// margen (2 x 32 bytes, en la `Feedback` y en la clave del resumen por operador) vuelve siempre al rater.
pub const DEPOSIT_OPERATOR_ID_LEN: usize = 64;
pub const MAX_SCORE: u8 = 100;
/// Tope de `limit` en las vistas paginadas (y su valor por defecto).
pub const MAX_LIST_LIMIT: u32 = 50;
/// Tope de bytes de la respuesta de `get_payment` (un `Payment` de v1 o v2 ocupa menos de 1,5 KiB).
pub const MAX_PAYMENT_RESULT_LEN: usize = 8 * 1024;
/// Gas de `escrow.get_payment` (una vista de un pago).
pub const GAS_GET_PAYMENT: Gas = Gas::from_tgas(10);
/// Gas reservado para `on_payment_checked`.
pub const GAS_ON_PAYMENT_CHECKED: Gas = Gas::from_tgas(30);
/// Gas minimo de `give_feedback`: la llamada cruzada, el callback y el propio recibo.
pub const MIN_GAS_GIVE_FEEDBACK: Gas = Gas::from_tgas(60);
/// Un candado que lleva este numero de bloques sin callback (porque el callback se quedo sin gas) se puede pisar con
/// otro `give_feedback` del mismo rater y pago. El callback viejo, si llegara despues, ve otro `nonce` y rechaza
/// (`E26`): nunca hay dos calificaciones.
pub const PENDING_TTL_BLOCKS: u64 = 1_000;
/// Bytes que el protocolo suma por cada registro de storage (`num_extra_bytes_record`).
pub const RECORD_OVERHEAD_BYTES: u64 = 40;
/// Largo maximo de un `AccountId` de NEAR.
pub const MAX_ACCOUNT_ID_LEN: u64 = 64;

/// Prefijos de storage de un byte.
pub mod prefix {
    pub const AGENTS: &[u8] = b"a";
    pub const FEEDBACKS: &[u8] = b"f";
    pub const FEEDBACK_BY_KEY: &[u8] = b"k";
    pub const RECEIVED: &[u8] = b"r";
    /// Los `Vector<u32>` de cada calificada: `i` + borsh(account_id) + `/`.
    pub const RECEIVED_INDEX: &[u8] = b"i";
    pub const SUMMARIES: &[u8] = b"s";
    /// Resumen por `(ratee, operator_id)` (RESPUESTA de c0der del 2026-11-04 00:06Z).
    pub const SUMMARIES_BY_OPERATOR: &[u8] = b"o";
    pub const PENDING: &[u8] = b"p";
}

/// El rol de la **calificada** en el pago.
#[near(serializers = [borsh, json])]
#[serde(rename_all = "snake_case")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// El agente que pago (lo califica el receiver).
    Payer,
    /// El worker que cobro (lo califica el payer).
    Receiver,
}

/// La ficha de una cuenta registrada.
#[near(serializers = [borsh])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Agent {
    pub agent_uri: String,
    pub metadata: Option<String>,
    pub registered_at: u64,
    pub updated_at: u64,
}

/// Una calificacion. `revoked`/`revoked_at` son de largo fijo: revocar no cambia el storage.
#[near(serializers = [borsh])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Feedback {
    pub rater: AccountId,
    pub ratee: AccountId,
    pub ratee_role: Role,
    pub escrow: AccountId,
    pub payment_id: PaymentId,
    pub operator_id: String,
    pub released: u128,
    pub score: u8,
    pub tag1: Option<String>,
    pub tag2: Option<String>,
    pub feedback_uri: Option<String>,
    pub feedback_hash: Option<[u8; 32]>,
    pub created_at: u64,
    pub revoked: bool,
    pub revoked_at: u64,
}

/// Contadores de un rol. `count` y `score_sum` son de las vigentes; `revoked`, de las revocadas.
#[near(serializers = [borsh])]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RoleStats {
    pub count: u32,
    pub revoked: u32,
    pub score_sum: u64,
}

/// Resumen de una calificada; existe desde su primera calificacion.
#[near(serializers = [borsh])]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    pub as_payer: RoleStats,
    pub as_receiver: RoleStats,
    /// La ultima recibida (vigente o no).
    pub last_feedback_id: u32,
    pub last_at: u64,
}

impl Summary {
    pub fn role_mut(&mut self, role: Role) -> &mut RoleStats {
        match role {
            Role::Payer => &mut self.as_payer,
            Role::Receiver => &mut self.as_receiver,
        }
    }

    /// Suma una calificacion vigente y la marca como la ultima.
    pub(crate) fn add(&mut self, role: Role, score: u8, id: u32, at: u64) {
        let r = self.role_mut(role);
        r.count = r.count.saturating_add(1);
        r.score_sum = r.score_sum.saturating_add(u64::from(score));
        self.last_feedback_id = id;
        self.last_at = at;
    }

    /// Pasa una calificacion de vigente a revocada.
    pub(crate) fn revoke(&mut self, role: Role, score: u8) {
        let r = self.role_mut(role);
        r.count = r.count.saturating_sub(1);
        r.revoked = r.revoked.saturating_add(1);
        r.score_sum = r.score_sum.saturating_sub(u64::from(score));
    }
}

/// Contrato `Registry`.
#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Registry {
    /// Fija desde `new`: los contratos de escrow de Arras cuyos pagos valen para calificar.
    escrows: Vec<AccountId>,
    agents: IterableMap<AccountId, Agent>,
    /// El indice es el `feedback_id`.
    feedbacks: Vector<Feedback>,
    /// sha256(borsh((escrow, payment_id, rater))) -> `feedback_id`.
    feedback_by_key: LookupMap<[u8; 32], u32>,
    /// Calificada -> sus `feedback_id`, en orden de llegada.
    received: LookupMap<AccountId, Vector<u32>>,
    summaries: LookupMap<AccountId, Summary>,
    /// El mismo resumen, por `(ratee, operator_id)` del pago: EM consulta el suyo (`operator_id == "em"`). La clave no
    /// lleva el escrow: vale porque la lista de `new` es un solo escrow para siempre (`v1.arras-escrow.near`); un
    /// registro con varios escrows tendria que sumarlo.
    summaries_by_operator: LookupMap<(AccountId, String), Summary>,
    /// Candado de `give_feedback` en vuelo: misma clave -> `block_height` del `give_feedback` (es el `nonce`).
    pending: LookupMap<[u8; 32], u64>,
}

/// La clave de una calificacion: sha256 de borsh((escrow, payment_id, rater)) (borsh de un String o un AccountId lleva
/// su largo adelante: la tupla no es ambigua).
pub fn feedback_key(escrow: &AccountId, payment_id: &str, rater: &AccountId) -> [u8; 32] {
    let bytes = borsh::to_vec(&(escrow, payment_id, rater)).unwrap_or_else(|_| env::abort());
    env::sha256_array(&bytes)
}

/// Prefijo del `Vector` de calificaciones recibidas: `i` + borsh(account_id) + `/`.
pub(crate) fn received_prefix(account: &AccountId) -> Vec<u8> {
    let mut p = prefix::RECEIVED_INDEX.to_vec();
    p.extend_from_slice(&(account.len() as u32).to_le_bytes());
    p.extend_from_slice(account.as_bytes());
    p.push(b'/');
    p
}

/// Bytes de un registro de storage: overhead + clave + valor.
pub const fn record_bytes(key_len: u64, value_len: u64) -> u64 {
    RECORD_OVERHEAD_BYTES + key_len + value_len
}

/// borsh de un `AccountId` de largo maximo / de un `String` de `n` bytes / de un `Option<String>` de `n` bytes.
const ACCOUNT_MAX: u64 = 4 + MAX_ACCOUNT_ID_LEN;
const fn string_bytes(n: usize) -> u64 {
    4 + n as u64
}
const fn opt_string_bytes(n: usize) -> u64 {
    1 + string_bytes(n)
}

/// Peor caso de lo que escribe una calificacion aceptada: la `Feedback` con todo al tope, su entrada de
/// `feedback_by_key`, la entrada nueva de `received` (el `Vector` de una calificada nueva), su elemento, el `Summary`
/// nuevo y el `Summary` nuevo de `(ratee, operator_id)`, con un `operator_id` de `DEPOSIT_OPERATOR_ID_LEN` (64) bytes.
/// `feedback_deposit` = este numero por el costo del byte; el callback cobra lo medido y devuelve el resto. Como v1
/// limita el `operator_id` a 32, el peor caso real es `FEEDBACK_MAX_BYTES - 64` (1.800); U-ST1 lo mide exacto.
pub const FEEDBACK_MAX_BYTES: u64 = {
    let feedback_value = ACCOUNT_MAX * 3 // rater, ratee, escrow
        + 1 // ratee_role
        + string_bytes(MAX_PAYMENT_ID_LEN)
        + string_bytes(DEPOSIT_OPERATOR_ID_LEN)
        + 16 // released
        + 1 // score
        + opt_string_bytes(MAX_TAG_LEN) * 2
        + opt_string_bytes(MAX_FEEDBACK_URI_LEN)
        + 1 + 32 // feedback_hash
        + 8 // created_at
        + 1 + 8; // revoked, revoked_at
    let feedback = record_bytes(1 + 4, feedback_value);
    let by_key = record_bytes(1 + 32, 4);
    let index_prefix = 1 + ACCOUNT_MAX + 1;
    let received = record_bytes(1 + ACCOUNT_MAX, 4 + 4 + index_prefix);
    let received_elem = record_bytes(index_prefix + 4, 4);
    let summary = record_bytes(1 + ACCOUNT_MAX, 16 * 2 + 4 + 8);
    let summary_by_operator = record_bytes(
        1 + ACCOUNT_MAX + string_bytes(DEPOSIT_OPERATOR_ID_LEN),
        16 * 2 + 4 + 8,
    );
    feedback + by_key + received + received_elem + summary + summary_by_operator
};

/// Peor caso de lo que escribe `register`: la clave en el `Vector` de `IterableMap` (`av` + u32 -> borsh(account)) y
/// la ficha con su indice en el `LookupMap` interno, cuya clave es un sha256 (el hasher por defecto de
/// `store::IterableMap` es `Sha256`). Solo para el deposito recomendado de `get_config`; `register` cobra lo medido.
/// U-ST3 mide que el peor caso real es exactamente este.
pub const AGENT_MAX_BYTES: u64 = {
    let key_entry = record_bytes(2 + 4, ACCOUNT_MAX);
    let agent_value = string_bytes(MAX_AGENT_URI_LEN) + opt_string_bytes(MAX_METADATA_LEN) + 8 + 8;
    let value_entry = record_bytes(32, agent_value + 4);
    key_entry + value_entry
};

/// Costo en yocto de `bytes` de storage (sin desbordar: `u64::MAX * 10^19` entra en u128).
pub(crate) fn bytes_cost(bytes: u64) -> u128 {
    u128::from(bytes).saturating_mul(env::storage_byte_cost().as_yoctonear())
}

/// Devuelve `amount` yocto a `to` (si hay algo que devolver).
pub(crate) fn refund(to: &AccountId, amount: u128) {
    if amount > 0 {
        near_sdk::Promise::new(to.clone())
            .transfer(NearToken::from_yoctonear(amount))
            .detach();
    }
}

#[near]
impl Registry {
    /// Solo la propia cuenta (con `with-init-call` en el deploy): `#[private]` cierra el `new` ajeno. `escrows`: de 1
    /// a 8 cuentas, sin repetir y sin el propio registro (`E40`). La lista no cambia nunca.
    #[init]
    #[private]
    pub fn new(escrows: Vec<AccountId>) -> Self {
        let me = env::current_account_id();
        let repetido = escrows
            .iter()
            .enumerate()
            .any(|(i, a)| escrows[..i].contains(a));
        if escrows.is_empty() || escrows.len() > MAX_ESCROWS || repetido || escrows.contains(&me) {
            RegistryError::E40EscrowsInvalidos.panic();
        }
        Self {
            escrows,
            agents: IterableMap::new(prefix::AGENTS),
            feedbacks: Vector::new(prefix::FEEDBACKS),
            feedback_by_key: LookupMap::new(prefix::FEEDBACK_BY_KEY),
            received: LookupMap::new(prefix::RECEIVED),
            summaries: LookupMap::new(prefix::SUMMARIES),
            summaries_by_operator: LookupMap::new(prefix::SUMMARIES_BY_OPERATOR),
            pending: LookupMap::new(prefix::PENDING),
        }
    }
}

impl Registry {
    /// Persiste los caches de todas las colecciones (en la wasm lo hace el `Drop` al final de la llamada): lo usan
    /// las mediciones de storage con `env::storage_usage()`.
    pub(crate) fn flush_all(&mut self) {
        self.agents.flush();
        self.feedbacks.flush();
        self.feedback_by_key.flush();
        self.received.flush();
        self.summaries.flush();
        self.summaries_by_operator.flush();
        self.pending.flush();
    }

    /// Lo que cuesta, en el peor caso, una calificacion (lo que `give_feedback` exige adjuntar).
    pub fn feedback_deposit() -> u128 {
        bytes_cost(FEEDBACK_MAX_BYTES)
    }
}
