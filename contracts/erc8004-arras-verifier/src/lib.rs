//! Verificador de pruebas de feedback de ERC-8004 en NEAR para pagos de Arras (seccion 3.3 del plan, opcion A elegida
//! por el dueno). Va en `erc8004-verifier.arras-escrow.near` con `new({"escrow": "v1.arras-escrow.near"})`: UN escrow,
//! fijo, contratos y no personas. Inmutable, sin estado que cambie, sin deposito.
//!
//! `verify_feedback_proof(client, proof)`: `proof` es el `payment_id` canonico (base58 de 32 bytes). Llama
//! `escrow.get_payment(proof)` y en su callback exige el espejo estricto del `Payment` de Arras v1 (el de registry-v1,
//! RONDA 1, decision 5: los 16 campos exactos, numeros canonicos, `status` de v1, `operator_id` del charset de v1),
//! `released > 0` (cualquiera sea el estado), `payer != receiver`, y que `client` sea el payer o el receiver. Devuelve la
//! otra parte: `{"counterparty", "role" (el de la contraparte), "context": operator_id, "weight": released}`. Si algo no
//! cumple, **entra en panico**: el Reputation Registry ve la promesa fallida (`E45`) y devuelve el deposito.
//!
//! Cualquiera puede llamarlo (es una lectura con un callback); el Reputation Registry es quien ata `client` al que
//! califica.

use arras_escrow_core::{PaymentId, is_valid_operator_id};
use erc8004_common::types::FeedbackProof;
use erc8004_common::validate::is_canonical_uint;
use near_sdk::json_types::{U64, U128};
use near_sdk::serde::de::Error as _;
use near_sdk::serde::{Deserialize, Deserializer, Serialize};
use near_sdk::{
    AccountId, Gas, PanicOnDefault, Promise, PromiseError, bs58, env, ext_contract, near,
    serde_json,
};
use std::str::FromStr;

#[cfg(test)]
mod unit_tests;

/// Gas de `escrow.get_payment` (una vista de un pago; mide ~1,5 TGas en v1).
pub const GAS_GET_PAYMENT: Gas = Gas::from_tgas(10);
/// Gas reservado para `on_payment`.
pub const GAS_ON_PAYMENT: Gas = Gas::from_tgas(10);
/// Gas minimo de `verify_feedback_proof`. El Reputation Registry le da `GAS_VERIFY_PROOF` (50 TGas).
pub const MIN_GAS_VERIFY: Gas = Gas::from_tgas(25);
/// Tope de bytes de la respuesta de `get_payment` (un `Payment` de v1 ocupa menos de 1,5 KiB).
pub const MAX_PAYMENT_RESULT_LEN: usize = 8 * 1024;

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

/// La vista `get_payment` de Arras v1 (`contracts/escrow-v1/src/views.rs`). El trait solo genera `ext_escrow`.
#[allow(dead_code)]
#[ext_contract(ext_escrow)]
pub trait ArrasEscrow {
    fn get_payment(&self, payment_id: PaymentId) -> Option<serde_json::Value>;
}

/// Espejo exacto del `Payment` de Arras v1 (`contracts/escrow-v1/src/lib.rs:82-105`, los 16 campos), copiado de
/// `contracts/registry-v1/src/reputation.rs` (RONDA 1 de c0der, P2-1 (a)): falla cerrado. Un campo de mas o de menos,
/// una clave repetida, un tipo JSON distinto del de v1, un numero no canonico, un `status` que no es de v1 o un
/// `operator_id` fuera de `[a-z0-9_-]{1,32}` no deserializa.
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
    pub terms_hash: String,
    pub status: EscrowStatus,
    #[serde(deserialize_with = "canonical")]
    pub storage_charged: u128,
    pub sponsored: bool,
}

/// Los 5 estados de v1, solo como el string que serializa v1. El estado no decide nada (decide `released`).
#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(crate = "near_sdk::serde", try_from = "String")]
pub enum EscrowStatus {
    Authorized,
    PartiallyReleased,
    Released,
    Refunded,
    Reclaimed,
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
            _ => Err("status que no es de v1"),
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

fn operator<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let s = String::deserialize(d)?;
    if !is_valid_operator_id(&s) {
        return Err(D::Error::custom("operator_id fuera del charset de v1"));
    }
    Ok(s)
}

/// Un `payment_id` canonico: base58 de exactamente 32 bytes, y la misma cadena al volver a codificarlo.
pub fn is_canonical_payment_id(s: &str) -> bool {
    if s.is_empty() || s.len() > 44 {
        return false;
    }
    match bs58::decode(s).into_vec() {
        Ok(b) => b.len() == 32 && bs58::encode(&b).into_string() == s,
        Err(_) => false,
    }
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

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(crate = "near_sdk::serde")]
pub struct VerifierConfig {
    pub contract: String,
    pub escrow: AccountId,
    pub min_gas_verify: U64,
    pub gas_get_payment: U64,
    pub gas_on_payment: U64,
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct ArrasVerifier {
    /// Fijo desde `new`.
    escrow: AccountId,
}

#[near]
impl ArrasVerifier {
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
            contract: "erc8004-arras-verifier".into(),
            escrow: self.escrow.clone(),
            min_gas_verify: U64(MIN_GAS_VERIFY.as_gas()),
            gas_get_payment: U64(GAS_GET_PAYMENT.as_gas()),
            gas_on_payment: U64(GAS_ON_PAYMENT.as_gas()),
        }
    }
}
