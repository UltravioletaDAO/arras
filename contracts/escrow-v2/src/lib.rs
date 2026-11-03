//! Arras v2: el contrato v1 (prescripcion sec. 3) mas lo de la sec. 4, en una cuenta propia, inmutable, sin
//! `migrate` y sin llaves despues del deploy:
//!
//! - **Presupuesto de agente** ([`budget`]): el humano deposita USDC con `ft_transfer_call` (`msg` `budget`) para un
//!   agente; el agente autoriza pagos con `authorize_from_budget` (deposit 0, asi lo firma una function-call key
//!   limitada a este contrato), con tope por pago y tope por ventana diaria. Lo que el agente autoriza queda en
//!   escrow como cualquier pago: solo el capturer del operador lo libera al receiver, y un refund vuelve al humano.
//! - **Disputa** ([`dispute`]): el payer o el receiver abren una disputa antes de `dispute_deadline`; solo el arbitro
//!   del operador la resuelve (reparto libre de lo vivo) dentro de su plazo; vencido el plazo, cualquiera la cierra
//!   al ultimo estado acordado: lo liberado queda liberado y lo vivo va a la parte que no abrio la disputa.
//! - **Ticket del operador** ([`ticket`]): si el operador tiene `ticket_pubkey`, todo pago y todo deposito de
//!   presupuesto en ese operador necesita su firma ed25519 (contra el griefing del pool de storage y del TVL).
//!
//! Lo comun con v1 (tipos, `payment_id`, tarifas, codigos y eventos de v1) viene de `arras-escrow-core` sin cambios.
//! Modelo pull, TVL e invariante como v1, con los presupuestos adentro:
//! `sum(vivos) + sum(claimable) + in_flight + sum(presupuestos) == tvl_global <= ft_balance_of(contrato)`.

use arras_escrow_core::ArrasError;
use near_sdk::json_types::{U64, U128};
use near_sdk::store::{IterableMap, LookupMap, LookupSet, Vector};
use near_sdk::{AccountId, Gas, NearToken, PanicOnDefault, PublicKey, env, near};

mod admin;
mod authorize;
mod budget;
mod dispute;
pub mod error;
pub mod events;
mod lifecycle;
pub mod msg;
mod storage;
pub mod ticket;
mod views;
mod withdraw;

pub use arras_escrow_core::{MAX_FEE_BPS, PaymentId};
pub use error::ArrasErrorV2;
pub use views::{BudgetView, ConfigView, StorageHealth};

#[cfg(test)]
mod unit_tests;

/// `get_config.version`.
pub const VERSION: &str = "arras-v2";
/// Gas del `ft_transfer` de cada retiro (igual que v1).
pub const GAS_FT_TRANSFER: Gas = Gas::from_tgas(10);
/// Gas reservado para `on_withdraw` (igual que v1).
pub const GAS_ON_WITHDRAW: Gas = Gas::from_tgas(10);
/// Retencion antes de `compact`: 7 dias despues de `reclaim_after` (o de la resolucion de la disputa, si es mas
/// tarde).
pub const RETENTION_NS: u64 = 7 * DAY_NS;
/// `get_storage_health` alarma si la reserva libre baja de 0,2 NEAR.
pub const MIN_FREE_RESERVE: NearToken = NearToken::from_millinear(200);
/// Tope de `limit` en `list_payments`.
pub const MAX_LIST_LIMIT: u32 = 50;
/// Bytes que el protocolo suma por cada registro de storage (`num_extra_bytes_record`).
pub const RECORD_OVERHEAD_BYTES: u64 = 40;
/// Largo maximo de un `AccountId` de NEAR.
pub const MAX_ACCOUNT_ID_LEN: u64 = 64;
/// Un dia en ns: la ventana del `tope_diario` es el dia UTC (`now / DAY_NS`).
pub const DAY_NS: u64 = 86_400 * 1_000_000_000;
/// Minimo de `arbiter_window_ns`: un minuto.
pub const MIN_ARBITER_WINDOW_NS: u64 = 60 * 1_000_000_000;
/// Maximo de `arbiter_window_ns`: 30 dias (acota cuanto puede quedar trabada una disputa sin arbitro).
pub const MAX_ARBITER_WINDOW_NS: u64 = 30 * DAY_NS;
/// Horizonte de un pago del presupuesto: `reclaim_after <= now + 30 dias` (`E57`). Lo pone el contrato porque los plazos
/// los elige el agente por el owner (refutacion de v2, P1-1).
pub const MAX_PLAZO_PRESUPUESTO_NS: u64 = 30 * DAY_NS;
/// Lo que crece un `Payment` al abrir y resolver su disputa: `Some(Dispute)` con `resolved_at` (87 bytes) menos
/// `None` (1). Se cobra al autorizar (quien abre la disputa no adjunta deposito) y se devuelve al compactar si la
/// disputa nunca se abrio.
pub const DISPUTE_RESERVE_BYTES: u64 = 86;

/// `operator_id`: `[a-z0-9_-]{1,32}`.
pub type OperatorId = String;
/// Clave de un presupuesto: `(owner, agent, operator_id)`. El token es el del contrato (v2 es de un solo token,
/// como v1).
pub type BudgetKey = (AccountId, AccountId, OperatorId);

/// Prefijos de storage de un byte: los de v1 mas `b` (presupuestos).
pub mod prefix {
    pub const OPERATORS: &[u8] = b"o";
    pub const PAYMENTS: &[u8] = b"p";
    pub const PAYMENTS_BY_OPERATOR: &[u8] = b"i";
    pub const USED_IDS: &[u8] = b"u";
    pub const CLAIMABLE: &[u8] = b"c";
    pub const PAYER_TVL: &[u8] = b"v";
    pub const STORAGE: &[u8] = b"s";
    pub const SPONSORED_USED: &[u8] = b"z";
    pub const BUDGETS: &[u8] = b"b";
}

/// Estado de un pago: los de v1 y, al final, `Disputed` y `Resolved` (sec. 3.1).
#[near(serializers = [borsh, json])]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaymentStatus {
    Authorized,
    PartiallyReleased,
    Released,
    Refunded,
    Reclaimed,
    /// Lo vivo esta congelado: solo `resolve_dispute` (el arbitro) o `resolve_expired_dispute` lo mueven.
    Disputed,
    /// Terminal: la disputa se repartio.
    Resolved,
}

impl PaymentStatus {
    /// `Authorized` o `PartiallyReleased`: tiene plata viva que release, refund y reclaim pueden mover.
    pub fn is_live(&self) -> bool {
        matches!(self, Self::Authorized | Self::PartiallyReleased)
    }

    /// Ya no se mueve nada: se puede compactar.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Released | Self::Refunded | Self::Reclaimed | Self::Resolved
        )
    }
}

/// Quien abrio la disputa.
#[near(serializers = [borsh, json])]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Party {
    Payer,
    Receiver,
}

/// La disputa de un pago.
#[near(serializers = [borsh, json])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dispute {
    pub opened_by: Party,
    /// Hex de 64 en minusculas (p. ej. sha256 del motivo, que vive fuera de la cadena).
    pub reason_hash: String,
    pub opened_at: U64,
    pub resolved_at: Option<U64>,
}

/// Valor de `payments`: el `Payment` de v1 mas los campos de la disputa.
#[near(serializers = [borsh, json])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Payment {
    pub operator_id: OperatorId,
    /// `sender_id` de `ft_on_transfer`, o el `owner` del presupuesto en `authorize_from_budget`.
    pub payer: AccountId,
    pub receiver: AccountId,
    pub amount: U128,
    pub released: U128,
    pub refunded: U128,
    /// Congelada al autorizar = `operator.fee_bps`.
    pub fee_bps: u16,
    /// Congelado al autorizar.
    pub fee_recipient: AccountId,
    /// Firmado por el payer (o por el agente, en el presupuesto).
    pub fee_bps_max: u16,
    pub created_at: U64,
    pub release_deadline: U64,
    pub reclaim_after: U64,
    /// Hex de 64 en minusculas.
    pub terms_hash: String,
    pub status: PaymentStatus,
    /// Yocto cobrados al payer (NEP-145) o al patrocinio, con la reserva de la disputa; vuelven al compactar.
    pub storage_charged: U128,
    /// Si lo pago el `sponsor_pool` del operador.
    pub sponsored: bool,
    /// Ultimo instante (exclusivo) para abrir una disputa: `now < dispute_deadline <= reclaim_after`.
    pub dispute_deadline: U64,
    /// Plazo del arbitro, congelado al autorizar (como la tarifa): `update_operator` no lo cambia para pagos vivos.
    pub arbiter_window_ns: U64,
    pub dispute: Option<Dispute>,
}

impl Payment {
    /// `amount - released - refunded`: lo que sigue en escrow (vivo o congelado en disputa).
    pub fn live_amount(&self) -> u128 {
        self.amount
            .0
            .checked_sub(self.released.0)
            .and_then(|r| r.checked_sub(self.refunded.0))
            .unwrap_or_else(|| env::abort())
    }
}

/// Valor de `operators`: el `Operator` de v1 mas el arbitro y el ticket.
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
    /// Lo vivo (incluido lo congelado en disputa) mas los presupuestos del operador.
    pub tvl_operador: U128,
    pub tope_tvl_por_payer: U128,
    /// Yocto para patrocinar storage de payers sin NEP-145.
    pub sponsor_pool: U128,
    /// Yocto de patrocinio por payer.
    pub sponsor_max_por_payer: U128,
    /// El unico que resuelve disputas dentro del plazo (el actual: `update_operator` lo puede rotar).
    pub arbiter: AccountId,
    /// Plazo del arbitro para los pagos nuevos, en `[1 minuto, 30 dias]`.
    pub arbiter_window_ns: U64,
    /// Si esta, todo pago y todo deposito de presupuesto en este operador necesita un ticket firmado con ella.
    pub ticket_pubkey: Option<PublicKey>,
}

impl Operator {
    /// Los 32 bytes de `ticket_pubkey` (siempre ed25519: `register_operator` y `update_operator` lo exigen).
    pub fn ticket_key_bytes(&self) -> Option<[u8; 32]> {
        self.ticket_pubkey.as_ref().map(|pk| {
            let b = pk.as_bytes();
            let mut k = [0u8; 32];
            if b.len() != 33 {
                env::abort();
            }
            k.copy_from_slice(&b[1..]);
            k
        })
    }
}

/// Presupuesto de un agente, por `(owner, agent, operator_id)`.
#[near(serializers = [borsh, json])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Budget {
    /// Lo que el agente todavia puede autorizar.
    pub saldo: U128,
    pub tope_por_pago: U128,
    pub tope_diario: U128,
    /// Lo autorizado en el dia `dia` (se reinicia al cambiar de dia UTC).
    pub gastado_hoy: U128,
    /// `now / DAY_NS` del ultimo gasto.
    pub dia: U64,
    /// Yocto cobrados al owner (NEP-145) o al patrocinio por el registro; vuelven con `revoke_agent`.
    pub storage_charged: U128,
    pub sponsored: bool,
}

/// Saldo NEP-145 de una cuenta (igual que v1).
#[near(serializers = [borsh])]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StorageAccount {
    pub total: u128,
    pub available: u128,
}

/// `register_operator`: los campos de v1 mas `arbiter`, `arbiter_window_ns` y `ticket_pubkey` (opcional).
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
    pub arbiter: AccountId,
    pub arbiter_window_ns: U64,
    #[serde(default)]
    pub ticket_pubkey: Option<PublicKey>,
}

/// `update_operator`: los editables, en `Option`. Los topes solo bajan (`E42`). `ticket_pubkey` pone o rota la
/// llave; `quitar_ticket_pubkey: true` la quita (las dos juntas: `E48`).
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
    pub arbiter: Option<AccountId>,
    pub arbiter_window_ns: Option<U64>,
    pub ticket_pubkey: Option<PublicKey>,
    pub quitar_ticket_pubkey: Option<bool>,
}

/// `operator_ticket` como argumento de `authorize_from_budget`: el mismo JSON que en el `msg`.
#[near(serializers = [json])]
#[serde(deny_unknown_fields)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TicketArg {
    pub expiry: U64,
    /// Base58 de los 64 bytes de la firma ed25519.
    pub sig: String,
}

impl From<TicketArg> for msg::OperatorTicket {
    fn from(t: TicketArg) -> Self {
        Self {
            expiry: t.expiry.0,
            sig: t.sig,
        }
    }
}

/// Contrato `Arras` v2.
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
    /// Suma de los `available` de NEP-145.
    storage_available_total: u128,
    /// Suma de los `saldo` de todos los presupuestos.
    budgets_total: u128,
    /// Yocto de la reserva de disputa ya cobrada y todavia sin usar (pagos sin disputa y sin compactar):
    /// `get_storage_health` no la cuenta como libre (refutacion de v2, P3-5).
    dispute_reserve_total: u128,
    operators: IterableMap<OperatorId, Operator>,
    payments: IterableMap<PaymentId, Payment>,
    payments_by_operator: LookupMap<OperatorId, Vector<PaymentId>>,
    used_ids: LookupSet<[u8; 32]>,
    /// Una entrada por cuenta; **nunca se borra**.
    claimable: LookupMap<AccountId, u128>,
    /// Lo vivo mas los presupuestos de cada `(operator_id, payer)`; el payer de un presupuesto es su owner.
    payer_tvl: LookupMap<(OperatorId, AccountId), u128>,
    storage: LookupMap<AccountId, StorageAccount>,
    sponsored_used: LookupMap<(OperatorId, AccountId), u128>,
    budgets: LookupMap<BudgetKey, Budget>,
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

/// `a + b` sin desbordar (desborde = abort: no pasa con montos reales).
pub(crate) fn add(a: u128, b: u128) -> u128 {
    a.checked_add(b).unwrap_or_else(|| env::abort())
}

/// `a - b` sin desbordar.
pub(crate) fn sub(a: u128, b: u128) -> u128 {
    a.checked_sub(b).unwrap_or_else(|| env::abort())
}

#[near]
impl Arras {
    /// Solo la propia cuenta (con `with-init-call` en el deploy), como v1.
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
            budgets_total: 0,
            dispute_reserve_total: 0,
            operators: IterableMap::new(prefix::OPERATORS),
            payments: IterableMap::new(prefix::PAYMENTS),
            payments_by_operator: LookupMap::new(prefix::PAYMENTS_BY_OPERATOR),
            used_ids: LookupSet::new(prefix::USED_IDS),
            claimable: LookupMap::new(prefix::CLAIMABLE),
            payer_tvl: LookupMap::new(prefix::PAYER_TVL),
            storage: LookupMap::new(prefix::STORAGE),
            sponsored_used: LookupMap::new(prefix::SPONSORED_USED),
            budgets: LookupMap::new(prefix::BUDGETS),
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

    #[cfg(test)]
    pub(crate) fn payer_tvl_of(&self, operator_id: &str, payer: &AccountId) -> u128 {
        self.payer_tvl
            .get(&(operator_id.to_string(), payer.clone()))
            .copied()
            .unwrap_or(0)
    }

    #[cfg(test)]
    pub(crate) fn sponsored_used_of(&self, operator_id: &str, payer: &AccountId) -> u128 {
        self.sponsored_used
            .get(&(operator_id.to_string(), payer.clone()))
            .copied()
            .unwrap_or(0)
    }

    #[cfg(test)]
    pub(crate) fn budget_of(&self, owner: &str, agent: &str, operator_id: &str) -> Option<Budget> {
        self.budgets
            .get(&(
                owner.parse().unwrap(),
                agent.parse().unwrap(),
                operator_id.to_string(),
            ))
            .cloned()
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
        self.budgets.flush();
    }
}
