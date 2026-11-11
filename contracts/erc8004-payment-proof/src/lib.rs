//! Verificador de pruebas de feedback de ERC-8004 en NEAR para los pagos del escrow de Arras con disputa
//! (`contracts/escrow-v2`, la cuenta `v2.arras-escrow.near`). Va en `payment-proof.arras-escrow.near` con
//! `new({"escrow": "v2.arras-escrow.near"})`: UN escrow, fijo, contratos y no personas. Inmutable, sin estado que
//! cambie, sin deposito. La misma interfaz y las mismas garantias que `contracts/erc8004-arras-verifier` (el de
//! `v1.arras-escrow.near`, desplegado y sin tocar), sobre el `Payment` de este escrow.
//!
//! `verify_feedback_proof(client, proof)`: `proof` es el `payment_id` canonico (base58 de 32 bytes). Llama
//! `escrow.get_payment(proof)` y en su callback exige el espejo estricto del `Payment` (los 19 campos exactos y los 4
//! de su `Dispute`, numeros canonicos, uno de los 7 `status`, el `status` coherente con `dispute` y
//! `released + refunded <= amount`), `released > 0` (cualquiera sea el estado), `payer != receiver`, y que `client` sea
//! el payer o el receiver. Devuelve la otra parte: `{"counterparty", "role" (el de la contraparte), "context":
//! operator_id, "weight": released}`. Si algo no cumple, **entra en panico**: el Reputation Registry ve la promesa
//! fallida (`E45`) y devuelve el deposito.
//!
//! **Disputas: valen, con `released > 0`.** Decidido con `contracts/escrow-v2/src/{lifecycle,dispute}.rs` en la mano:
//! `released` solo sube (`release` suma lo liberado; resolver una disputa suma `to_receiver`; `refund`, `reclaim` y
//! `open_dispute` no lo tocan) y cada suba acredita el `claimable` del receiver y la tarifa del fee recipient sin vuelta
//! atras: ninguna operacion del escrow lo devuelve. Asi que `released > 0` es plata que llego al receiver en cualquier
//! estado:
//!
//! - `Disputed` con `released > 0`: lo liberado antes de abrir la disputa (un `PartiallyReleased`) ya es del receiver;
//!   la disputa solo congela lo vivo. Vale con el `released` de ese momento (puede crecer al resolverse; el registro
//!   acepta una calificacion por `(verificador, proof, cliente)`, asi que el cliente elige cuando calificar).
//! - `Resolved`: `released` es lo liberado antes mas lo que la resolucion le dio al receiver, la decida el arbitro del
//!   operador o la regla del plazo vencido (el payer abrio antes de `release_deadline` y el arbitro no contesto). Vale
//!   si es > 0; todo devuelto al payer sin liberacion previa es `released == 0` (`E13`), como un refund.
//! - No se excluyen: cualquiera de las dos partes puede abrir una disputa mientras el pago esta vivo, y si `Disputed` o
//!   `Resolved` no valieran, un payer abriria una despues de una liberacion parcial para que el receiver no lo califique
//!   nunca (y al reves). La calificacion de un trato disputado es de las que mas informan.
//!
//! **`released > 0` no implica que actuo el operador.** La disputa vencida a favor del receiver sube `released` sin el
//! capturer ni el arbitro: un payer que se paga a si mismo (dos cuentas) abre la disputa antes de `release_deadline`,
//! espera el plazo del arbitro y cualquiera la cierra. Le cuesta lo mismo que una liberacion (la tarifa del operador),
//! pero no necesita al operador. Con `ticket_pubkey` en el operador (como `em` en mainnet) todo pago necesita su firma
//! y el costo es el de v1; sin ticket, `context` solo dice en que operador se autorizo el pago. El verificador podria
//! reconocer esa via (`resolved_at >= opened_at + arbiter_window_ns`), pero no separar lo liberado antes de lo que dio
//! el plazo, asi que no la rechaza: rechazarla le quitaria la calificacion al worker al que el payer le trabo el pago.
//!
//! Cualquiera puede llamarlo (es una lectura con un callback); el Reputation Registry es quien ata `client` al que
//! califica.

use arras_escrow_core::{PaymentId, is_valid_operator_id};
use erc8004_common::types::FeedbackProof;
use erc8004_common::validate::{is_canonical_uint, parse_hash};
use near_sdk::json_types::{U64, U128};
use near_sdk::serde::de::Error as _;
use near_sdk::serde::{Deserialize, Deserializer};
use near_sdk::{
    AccountId, Gas, PanicOnDefault, Promise, PromiseError, bs58, env, ext_contract, near,
    serde_json,
};
use std::str::FromStr;

#[cfg(test)]
mod unit_tests;

/// Gas de `escrow.get_payment` (una vista de un pago; lo mide WP-07 en el sandbox).
pub const GAS_GET_PAYMENT: Gas = Gas::from_tgas(10);
/// Gas reservado para `on_payment`.
pub const GAS_ON_PAYMENT: Gas = Gas::from_tgas(10);
/// Gas minimo de `verify_feedback_proof`. El Reputation Registry le da `GAS_VERIFY_PROOF` (50 TGas).
pub const MIN_GAS_VERIFY: Gas = Gas::from_tgas(25);
/// Tope de bytes de la respuesta de `get_payment` (un `Payment` con disputa ocupa menos de 1,5 KiB).
pub const MAX_PAYMENT_RESULT_LEN: usize = 8 * 1024;
/// `get_config.contract`: el mismo que el verificador de `v1.arras-escrow.near`. Es el nombre de la interfaz (los mismos
/// campos de `get_config`, los mismos codigos `ERC8004_ARV_*`, la misma respuesta) y los SDK lo comparan tal cual; lo
/// que distingue a los dos verificadores es `escrow`.
pub const CONTRACT_KIND: &str = "erc8004-arras-verifier";

/// Los codigos del verificador de v1, con el mismo prefijo `ERC8004_ARV_` (los SDK los leen de la falla). No hace falta
/// ninguno nuevo: lo que no es exactamente un `Payment` de este escrow, o no es coherente, es `E11`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifierError {
    E01EscrowInvalido,
    E02ProofInvalida,
    E03GasInsuficiente,
    E10GetPaymentFallo,
    E11RespuestaInvalida,
    E12PagoInexistente,
    E13PagoSinLiberar,
    E14PayerEsReceiver,
    E15NoEsParte,
}

impl VerifierError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::E01EscrowInvalido => "E01_ESCROW_INVALIDO",
            Self::E02ProofInvalida => "E02_PROOF_INVALIDA",
            Self::E03GasInsuficiente => "E03_GAS_INSUFICIENTE",
            Self::E10GetPaymentFallo => "E10_GET_PAYMENT_FALLO",
            Self::E11RespuestaInvalida => "E11_RESPUESTA_INVALIDA",
            Self::E12PagoInexistente => "E12_PAGO_INEXISTENTE",
            Self::E13PagoSinLiberar => "E13_PAGO_SIN_LIBERAR",
            Self::E14PayerEsReceiver => "E14_PAYER_ES_RECEIVER",
            Self::E15NoEsParte => "E15_NO_ES_PARTE",
        }
    }

    pub fn message(&self) -> &'static str {
        match self {
            Self::E01EscrowInvalido => "escrow invalido",
            Self::E02ProofInvalida => "proof invalida: payment_id canonico (base58 de 32 bytes)",
            Self::E03GasInsuficiente => "gas insuficiente para la llamada cruzada",
            Self::E10GetPaymentFallo => "get_payment del escrow fallo",
            Self::E11RespuestaInvalida => "respuesta de get_payment invalida",
            Self::E12PagoInexistente => "pago inexistente",
            Self::E13PagoSinLiberar => "el pago no libero plata (released == 0)",
            Self::E14PayerEsReceiver => "payer == receiver",
            Self::E15NoEsParte => "client no es el payer ni el receiver",
        }
    }

    pub fn panic_message(&self) -> String {
        format!("ERC8004_ARV_{}: {}", &self.code()[..3], self.message())
    }

    pub fn panic(&self) -> ! {
        env::panic_str(&self.panic_message())
    }
}

/// La vista `get_payment` del escrow (`contracts/escrow-v2/src/views.rs`). El trait solo genera `ext_escrow`.
#[allow(dead_code)]
#[ext_contract(ext_escrow)]
pub trait ArrasEscrow {
    fn get_payment(&self, payment_id: PaymentId) -> Option<serde_json::Value>;
}

/// Espejo exacto del `Payment` del escrow (`contracts/escrow-v2/src/lib.rs`, los 19 campos: los 16 de v1 mas
/// `dispute_deadline`, `arbiter_window_ns` y `dispute`): falla cerrado. Un campo de mas o de menos (tambien `dispute`:
/// tiene que estar, con `null` si no hay), una clave repetida, un tipo JSON distinto del que serializa el escrow, un
/// numero no canonico, un hash que no es hex de 64 en minusculas, un `status` que no es uno de los 7 o un
/// `operator_id` fuera de `[a-z0-9_-]{1,32}` no deserializa. Lo prueban `unit_tests` contra el `Payment` de verdad.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(crate = "near_sdk::serde", deny_unknown_fields)]
pub struct EscrowPayment {
    #[serde(deserialize_with = "operator")]
    pub operator_id: String,
    pub payer: AccountId,
    pub receiver: AccountId,
    #[serde(deserialize_with = "canonical")]
    pub amount: u128,
    #[serde(deserialize_with = "canonical")]
    pub released: u128,
    #[serde(deserialize_with = "canonical")]
    pub refunded: u128,
    pub fee_bps: u16,
    pub fee_recipient: AccountId,
    pub fee_bps_max: u16,
    #[serde(deserialize_with = "canonical")]
    pub created_at: u64,
    #[serde(deserialize_with = "canonical")]
    pub release_deadline: u64,
    #[serde(deserialize_with = "canonical")]
    pub reclaim_after: u64,
    #[serde(deserialize_with = "hash")]
    pub terms_hash: String,
    pub status: EscrowStatus,
    #[serde(deserialize_with = "canonical")]
    pub storage_charged: u128,
    pub sponsored: bool,
    #[serde(deserialize_with = "canonical")]
    pub dispute_deadline: u64,
    #[serde(deserialize_with = "canonical")]
    pub arbiter_window_ns: u64,
    /// Con `deserialize_with`, serde no lo da por `None` si falta: tiene que estar.
    #[serde(deserialize_with = "required_nullable")]
    pub dispute: Option<EscrowDispute>,
}

/// Espejo exacto de la `Dispute` del escrow (sus 4 campos; `resolved_at` tiene que estar, con `null` si no se
/// resolvio).
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(crate = "near_sdk::serde", deny_unknown_fields)]
pub struct EscrowDispute {
    pub opened_by: EscrowParty,
    #[serde(deserialize_with = "hash")]
    pub reason_hash: String,
    #[serde(deserialize_with = "canonical")]
    pub opened_at: u64,
    #[serde(deserialize_with = "nullable_canonical")]
    pub resolved_at: Option<u64>,
}

/// Los 7 estados del escrow, solo como el string que serializa. El estado no decide nada (decide `released`); solo
/// tiene que ser coherente con `dispute` ([`is_coherent`]).
#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(crate = "near_sdk::serde", try_from = "String")]
pub enum EscrowStatus {
    Authorized,
    PartiallyReleased,
    Released,
    Refunded,
    Reclaimed,
    Disputed,
    Resolved,
}

impl TryFrom<String> for EscrowStatus {
    type Error = &'static str;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        match s.as_str() {
            "Authorized" => Ok(Self::Authorized),
            "PartiallyReleased" => Ok(Self::PartiallyReleased),
            "Released" => Ok(Self::Released),
            "Refunded" => Ok(Self::Refunded),
            "Reclaimed" => Ok(Self::Reclaimed),
            "Disputed" => Ok(Self::Disputed),
            "Resolved" => Ok(Self::Resolved),
            _ => Err("status que no es del escrow"),
        }
    }
}

/// Quien abrio la disputa, solo como el string que serializa el escrow.
#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(crate = "near_sdk::serde", try_from = "String")]
pub enum EscrowParty {
    Payer,
    Receiver,
}

impl TryFrom<String> for EscrowParty {
    type Error = &'static str;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        match s.as_str() {
            "Payer" => Ok(Self::Payer),
            "Receiver" => Ok(Self::Receiver),
            _ => Err("opened_by que no es del escrow"),
        }
    }
}

fn canonical<'de, D: Deserializer<'de>, T: FromStr>(d: D) -> Result<T, D::Error> {
    let s = String::deserialize(d)?;
    if !is_canonical_uint(&s) {
        return Err(D::Error::custom("numero no canonico"));
    }
    s.parse()
        .map_err(|_| D::Error::custom("numero fuera de rango"))
}

fn nullable_canonical<'de, D: Deserializer<'de>>(d: D) -> Result<Option<u64>, D::Error> {
    match Option::<String>::deserialize(d)? {
        None => Ok(None),
        Some(s) if is_canonical_uint(&s) => s
            .parse()
            .map(Some)
            .map_err(|_| D::Error::custom("numero fuera de rango")),
        Some(_) => Err(D::Error::custom("numero no canonico")),
    }
}

fn required_nullable<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    Option::<T>::deserialize(d)
}

fn hash<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let s = String::deserialize(d)?;
    if parse_hash(&s).is_none() {
        return Err(D::Error::custom("hash que no es hex de 64 en minusculas"));
    }
    Ok(s)
}

fn operator<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let s = String::deserialize(d)?;
    if !is_valid_operator_id(&s) {
        return Err(D::Error::custom("operator_id fuera del charset del escrow"));
    }
    Ok(s)
}

/// Un `payment_id` canonico: base58 (alfabeto de bitcoin) de exactamente 32 bytes. La misma regla que el verificador
/// de v1: cada `1` adelante es un byte cero y el resto es un numero en base 58 sin ceros a la izquierda, asi que dos
/// cadenas distintas nunca dan los mismos 32 bytes (el Reputation Registry ya limita `proof` a 128 bytes).
pub fn is_canonical_payment_id(s: &str) -> bool {
    bs58::decode(s).into_vec().is_ok_and(|b| b.len() == 32)
}

/// Lo que el escrow nunca escribe: `dispute` presente si y solo si el estado es `Disputed` o `Resolved` (`open_dispute`
/// pone las dos cosas juntas), `resolved_at` presente si y solo si es `Resolved` (`settle_dispute` idem), y
/// `released + refunded <= amount` (`live_amount` aborta si no).
pub fn is_coherent(p: &EscrowPayment) -> bool {
    let estado = match (p.status, &p.dispute) {
        (EscrowStatus::Disputed, Some(d)) => d.resolved_at.is_none(),
        (EscrowStatus::Resolved, Some(d)) => d.resolved_at.is_some(),
        (EscrowStatus::Disputed | EscrowStatus::Resolved, None) => false,
        (_, d) => d.is_none(),
    };
    estado
        && p.released
            .checked_add(p.refunded)
            .is_some_and(|s| s <= p.amount)
}

/// Lo que el callback verifico, o el error.
pub fn check_payment(
    client: &AccountId,
    result: Result<Vec<u8>, PromiseError>,
) -> Result<FeedbackProof, VerifierError> {
    let bytes = match result {
        Ok(b) => b,
        Err(PromiseError::Failed) => return Err(VerifierError::E10GetPaymentFallo),
        Err(_) => return Err(VerifierError::E11RespuestaInvalida),
    };
    let payment: Option<EscrowPayment> =
        serde_json::from_slice(&bytes).map_err(|_| VerifierError::E11RespuestaInvalida)?;
    let pay = payment.ok_or(VerifierError::E12PagoInexistente)?;
    if !is_coherent(&pay) {
        return Err(VerifierError::E11RespuestaInvalida);
    }
    if pay.released == 0 {
        return Err(VerifierError::E13PagoSinLiberar);
    }
    if pay.payer == pay.receiver {
        return Err(VerifierError::E14PayerEsReceiver);
    }
    let (counterparty, role) = if client == &pay.payer {
        (pay.receiver, "receiver")
    } else if client == &pay.receiver {
        (pay.payer, "payer")
    } else {
        return Err(VerifierError::E15NoEsParte);
    };
    Ok(FeedbackProof {
        counterparty,
        role: role.into(),
        context: pay.operator_id,
        weight: U128(pay.released),
    })
}

/// `get_config`: los mismos campos que el verificador de v1.
#[near(serializers = [json])]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifierConfig {
    /// [`CONTRACT_KIND`].
    pub contract: String,
    pub escrow: AccountId,
    pub min_gas_verify: U64,
    pub gas_get_payment: U64,
    pub gas_on_payment: U64,
}

/// NEP-330 `standards`: `erc8004` 1.0.0, el lado verificador de la extension R-5 (seccion 7 del plan), como v1.
#[near(
    contract_state,
    contract_metadata(standard(standard = "erc8004", version = "1.0.0"))
)]
#[derive(PanicOnDefault)]
pub struct PaymentProofVerifier {
    /// Fijo desde `new`.
    escrow: AccountId,
}

#[near]
impl PaymentProofVerifier {
    /// Solo la propia cuenta (`with-init-call`). El escrow no puede ser la propia cuenta.
    #[init]
    #[private]
    pub fn new(escrow: AccountId) -> Self {
        if escrow == env::current_account_id() {
            VerifierError::E01EscrowInvalido.panic();
        }
        Self { escrow }
    }

    /// La prueba de que `client` tuvo un pago con plata liberada en el escrow: `proof` = `payment_id`. Devuelve la
    /// otra parte (ver el doc del crate) o falla.
    pub fn verify_feedback_proof(&self, client: AccountId, proof: String) -> Promise {
        if !is_canonical_payment_id(&proof) {
            VerifierError::E02ProofInvalida.panic();
        }
        if env::prepaid_gas() < MIN_GAS_VERIFY {
            VerifierError::E03GasInsuficiente.panic();
        }
        ext_escrow::ext(self.escrow.clone())
            .with_static_gas(GAS_GET_PAYMENT)
            .get_payment(proof)
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(GAS_ON_PAYMENT)
                    .on_payment(client),
            )
    }

    /// Callback de `verify_feedback_proof`: lee primero el resultado (fuera de un callback de verdad el host aborta).
    #[private]
    pub fn on_payment(&self, client: AccountId) -> FeedbackProof {
        let result = env::promise_result_checked(0, MAX_PAYMENT_RESULT_LEN);
        check_payment(&client, result).unwrap_or_else(|e| e.panic())
    }

    pub fn get_config(&self) -> VerifierConfig {
        VerifierConfig {
            contract: CONTRACT_KIND.into(),
            escrow: self.escrow.clone(),
            min_gas_verify: U64(MIN_GAS_VERIFY.as_gas()),
            gas_get_payment: U64(GAS_GET_PAYMENT.as_gas()),
            gas_on_payment: U64(GAS_ON_PAYMENT.as_gas()),
        }
    }
}
