//! ERC-8004 Reputation Registry en NEAR (`docs/plans/erc8004-near-port.md`, seccion 3), port de
//! `ReputationRegistryUpgradeable` 2.0.0 (`erc-8004/erc-8004-contracts` b9e466c). Inmutable: `new(identity_registry)`
//! privado en la misma tx del deploy, sin `migrate`, sin owner; el Identity Registry no cambia nunca.
//!
//! - `give_feedback` acepta feedback de **cualquier** cliente que no sea el dueno ni un aprobado del agente; el agente
//!   tiene que existir. Las dos cosas las dice el Identity Registry (`check_agent`) en una llamada cruzada, y el
//!   callback **falla cerrado**: si la vista falla, si la respuesta no es exactamente un `AgentCheck`, si es `null` o
//!   si el cliente esta autorizado, rechaza sin escribir nada y devuelve todo lo adjuntado.
//! - `feedback_index` empieza en 1 y cuenta por par `(agente, cliente)`, como en EVM. Se guardan `value`,
//!   `value_decimals`, `tag1`, `tag2` e `is_revoked`; `endpoint`, `feedback_uri` y `feedback_hash` solo van en el
//!   evento.
//! - `revoke_feedback` (el cliente), `append_response` (cualquiera), `get_summary` (con la lista de clientes
//!   obligatoria, anti-sybil) y las demas vistas de EVM.
//! - Con el feature `feedback-proofs` (por defecto): `give_feedback_with_proof(..., verifier, proof)`, la calificacion
//!   con una prueba en cadena de un trato con el agente (seccion 3.3 del plan).

use erc8004_common::storage::{ACCOUNT_MAX_BYTES, record_bytes, string_bytes};
use erc8004_common::validate::{MAX_PROOF_LEN, MAX_TAG_LEN};
use near_sdk::store::{LookupMap, Vector};
use near_sdk::{AccountId, Gas, PanicOnDefault, env, near};

mod error;
mod events;
mod feedback;
#[cfg(feature = "feedback-proofs")]
mod proof;
mod views;

pub use error::ReputationError;
pub use events::*;
pub use feedback::PendingFeedback;
#[cfg(feature = "feedback-proofs")]
pub use proof::{PENDING_RECORD_BYTES, StoredProof, proof_key};
pub use views::*;

#[cfg(test)]
mod unit_tests;

pub const MAX_ENDPOINT_LEN: usize = 1024;
/// `feedback_uri` y `response_uri` no se guardan (solo van en el evento).
pub const MAX_FEEDBACK_URI_LEN: usize = 4096;
pub const MAX_RESPONSE_URI_LEN: usize = 4096;
/// Tope (y valor por defecto cuando se pagina) de `limit`.
pub const MAX_LIST_LIMIT: u64 = 100;

/// Gas de `identity.check_agent`.
pub const GAS_CHECK_AGENT: Gas = Gas::from_tgas(5);
/// Gas reservado para `on_feedback_checked`.
pub const GAS_ON_FEEDBACK_CHECKED: Gas = Gas::from_tgas(20);
/// Gas minimo de `give_feedback`: la vista, el callback y el propio recibo.
pub const MIN_GAS_GIVE_FEEDBACK: Gas = Gas::from_tgas(40);
/// Gas de `verifier.verify_feedback_proof` (el de Arras llama a su escrow y tiene su callback).
pub const GAS_VERIFY_PROOF: Gas = Gas::from_tgas(50);
/// Gas reservado para `on_feedback_proof_checked`.
pub const GAS_ON_PROOF_CHECKED: Gas = Gas::from_tgas(30);
/// Gas minimo de `give_feedback_with_proof`.
pub const MIN_GAS_GIVE_FEEDBACK_WITH_PROOF: Gas = Gas::from_tgas(120);
/// Un candado de prueba que lleva este numero de bloques sin callback se puede pisar (el callback viejo, si llegara,
/// ve otro `nonce` y rechaza con `E44`), como en registry-v1.
pub const PENDING_TTL_BLOCKS: u64 = 1_000;

/// Prefijos de storage de un byte (y los de los `Vector` anidados, con su sufijo).
pub mod prefix {
    pub const FEEDBACK: &[u8] = b"f";
    pub const LAST_INDEX: &[u8] = b"l";
    pub const CLIENTS: &[u8] = b"c";
    /// `C` + `agent_id` (u64 LE): el `Vector` de clientes de un agente.
    pub const CLIENTS_LIST: u8 = b'C';
    pub const RESPONSE_COUNT: &[u8] = b"r";
    pub const RESPONDERS: &[u8] = b"R";
    /// `Q` + sha256(borsh((agent_id, cliente, feedback_index))): el `Vector` de respondedores de una calificacion.
    pub const RESPONDERS_LIST: u8 = b'Q';
    pub const PROOFS: &[u8] = b"p";
    pub const PROOF_USED: &[u8] = b"u";
    pub const VERIFIED: &[u8] = b"v";
    /// `V` + sha256(borsh((agent_id, verificador))).
    pub const VERIFIED_LIST: u8 = b'V';
    pub const PENDING: &[u8] = b"k";
}

/// Una calificacion: lo que EVM guarda (`Feedback`). `""` es "sin tag".
#[near(serializers = [borsh])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Feedback {
    pub value: i128,
    pub value_decimals: u8,
    pub is_revoked: bool,
    pub tag1: String,
    pub tag2: String,
}

/// Clave de una calificacion.
pub type FeedbackKey = (u64, AccountId, u64);

/// Peor caso de lo que escribe una calificacion aceptada del nucleo: la `Feedback` con los tags al tope y un cliente
/// de 64, su `last_index`, y el cliente nuevo de un agente nuevo (el `Vector` y su elemento). `feedback_deposit` = este
/// numero por el costo del byte; el callback cobra lo medido y devuelve el resto. U-ST1 lo mide exacto.
pub const FEEDBACK_MAX_BYTES: u64 = {
    let key = 1 + 8 + ACCOUNT_MAX_BYTES + 8;
    let feedback = record_bytes(key, 16 + 1 + 1 + string_bytes(MAX_TAG_LEN) * 2);
    let last_index = record_bytes(1 + 8 + ACCOUNT_MAX_BYTES, 8);
    let clients = record_bytes(1 + 8, 4 + 4 + 9);
    let client = record_bytes(9 + 4, ACCOUNT_MAX_BYTES);
    feedback + last_index + clients + client
};

/// Lo que agrega una prueba en el peor caso: la prueba guardada (verificador, `proof`, contraparte, `role` y `context`
/// al tope, `weight`), la marca de prueba usada, y el verificado nuevo de un `(agente, verificador)` nuevo.
pub const PROOF_MAX_BYTES: u64 = {
    let key = 1 + 8 + ACCOUNT_MAX_BYTES + 8;
    let label = string_bytes(erc8004_common::validate::MAX_LABEL_LEN);
    let proof = record_bytes(
        key,
        ACCOUNT_MAX_BYTES * 2 + string_bytes(MAX_PROOF_LEN) + label * 2 + 16,
    );
    let used = record_bytes(1 + 32, 16);
    let verified = record_bytes(1 + 8 + ACCOUNT_MAX_BYTES, 4 + 4 + 33);
    let elem = record_bytes(33 + 4, ACCOUNT_MAX_BYTES + 8);
    proof + used + verified + elem
};

/// Peor caso de una calificacion con prueba.
pub const FEEDBACK_PROOF_MAX_BYTES: u64 = FEEDBACK_MAX_BYTES + PROOF_MAX_BYTES;

/// Contrato `ReputationRegistry`. NEP-330 `standards`: `erc8004` 1.0.0 (seccion 7 del plan).
#[near(
    contract_state,
    contract_metadata(standard(standard = "erc8004", version = "1.0.0"))
)]
#[derive(PanicOnDefault)]
pub struct ReputationRegistry {
    /// Fijo desde `new`: el Identity Registry que dice que agentes existen y quien es su dueno.
    identity_registry: AccountId,
    feedback: LookupMap<FeedbackKey, Feedback>,
    last_index: LookupMap<(u64, AccountId), u64>,
    /// Los clientes de cada agente, en orden de su primera calificacion (`_clients` de EVM).
    clients: LookupMap<u64, Vector<AccountId>>,
    /// `(agente, cliente, indice, respondedor)` -> cuantas respuestas (`_responseCount`).
    response_count: LookupMap<(u64, AccountId, u64, AccountId), u64>,
    /// Los respondedores de cada calificacion, en orden (`_responders`).
    responders: LookupMap<FeedbackKey, Vector<AccountId>>,
    #[cfg(feature = "feedback-proofs")]
    proofs: LookupMap<FeedbackKey, StoredProof>,
    /// sha256(borsh((verificador, proof, cliente))) -> `(agente, indice)`.
    #[cfg(feature = "feedback-proofs")]
    proof_used: LookupMap<[u8; 32], (u64, u64)>,
    /// `(agente, verificador)` -> las calificaciones con prueba de ese verificador.
    #[cfg(feature = "feedback-proofs")]
    verified: LookupMap<(u64, AccountId), Vector<(AccountId, u64)>>,
    /// Candado de `give_feedback_with_proof` en vuelo: la clave de la prueba -> `block_height` (el `nonce`).
    #[cfg(feature = "feedback-proofs")]
    pending: LookupMap<[u8; 32], u64>,
}

#[near]
impl ReputationRegistry {
    /// Solo la propia cuenta (`with-init-call`). `identity_registry` no puede ser la propia cuenta (`E17`): es la raiz
    /// de confianza y no cambia nunca (seccion 7 del plan).
    #[init]
    #[private]
    pub fn new(identity_registry: AccountId) -> Self {
        if identity_registry == env::current_account_id() {
            ReputationError::E17IdentityRegistryInvalido.panic();
        }
        Self {
            identity_registry,
            feedback: LookupMap::new(prefix::FEEDBACK),
            last_index: LookupMap::new(prefix::LAST_INDEX),
            clients: LookupMap::new(prefix::CLIENTS),
            response_count: LookupMap::new(prefix::RESPONSE_COUNT),
            responders: LookupMap::new(prefix::RESPONDERS),
            #[cfg(feature = "feedback-proofs")]
            proofs: LookupMap::new(prefix::PROOFS),
            #[cfg(feature = "feedback-proofs")]
            proof_used: LookupMap::new(prefix::PROOF_USED),
            #[cfg(feature = "feedback-proofs")]
            verified: LookupMap::new(prefix::VERIFIED),
            #[cfg(feature = "feedback-proofs")]
            pending: LookupMap::new(prefix::PENDING),
        }
    }
}

/// Prefijo del `Vector` de clientes de un agente.
pub(crate) fn clients_prefix(agent_id: u64) -> Vec<u8> {
    let mut p = vec![prefix::CLIENTS_LIST];
    p.extend_from_slice(&agent_id.to_le_bytes());
    p
}

/// Prefijo del `Vector` de respondedores de una calificacion.
pub(crate) fn responders_prefix(key: &FeedbackKey) -> Vec<u8> {
    let bytes = near_sdk::borsh::to_vec(key).unwrap_or_else(|_| env::abort());
    let mut p = vec![prefix::RESPONDERS_LIST];
    p.extend_from_slice(&env::sha256_array(&bytes));
    p
}

impl ReputationRegistry {
    pub(crate) fn flush_all(&mut self) {
        self.feedback.flush();
        self.last_index.flush();
        self.clients.flush();
        self.response_count.flush();
        self.responders.flush();
        #[cfg(feature = "feedback-proofs")]
        {
            self.proofs.flush();
            self.proof_used.flush();
            self.verified.flush();
            self.pending.flush();
        }
    }

    pub(crate) fn usage(&mut self) -> u64 {
        self.flush_all();
        env::storage_usage()
    }

    pub(crate) fn last(&self, agent_id: u64, client: &AccountId) -> u64 {
        self.last_index
            .get(&(agent_id, client.clone()))
            .copied()
            .unwrap_or(0)
    }
}

/// `agent_id` canonico o `E01`.
pub(crate) fn parse_agent_id(s: &str) -> u64 {
    erc8004_common::validate::parse_u64(s)
        .unwrap_or_else(|| ReputationError::E01AgentIdInvalido.panic())
}

/// `feedback_index` canonico o `E11`.
pub(crate) fn parse_index(s: &str) -> u64 {
    erc8004_common::validate::parse_u64(s)
        .unwrap_or_else(|| ReputationError::E11FeedbackIndexInvalido.panic())
}
