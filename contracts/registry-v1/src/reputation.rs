//! Reputacion: `give_feedback` (con la llamada cruzada a `get_payment`), su callback `on_payment_checked` y
//! `revoke_feedback`.
//!
//! `give_feedback`, en orden (cada una entra en panico y el runtime devuelve el deposito): `E10` escrow fuera de la
//! lista -> `E11` payment_id -> `E12` score -> `E13` tags -> `E14` feedback_uri -> `E15` feedback_hash -> `E16`
//! deposito -> `E17` gas -> `E18` ya calificado -> `E19` en vuelo. Despues pone el candado (`pending`) y llama.
//!
//! `on_payment_checked` (nunca entra en panico): `E26` el candado ya no es de esta llamada -> `E20` get_payment fallo
//! -> `E21` respuesta de mas de 8 KiB o que no es exactamente un `Payment` de v1 (ver [`EscrowPayment`]) -> `E22`
//! `null` -> `E23` `released == 0` (no paso plata, cualquiera sea el estado) -> `E24` no es parte -> `E25` se
//! califica a si mismo -> `E18` ya calificado. Un rechazo devuelve el deposito entero; una calificacion aceptada cobra
//! el storage medido y devuelve el resto.

use crate::events::{FeedbackGivenData, FeedbackRevokedData};
use crate::validate::{
    check_feedback_uri, check_payment_id, check_tag, is_canonical_uint, parse_hash,
};
use crate::*;
use arras_escrow_core::is_valid_operator_id;
use near_sdk::serde::de::Error as _;
use near_sdk::serde::{Deserialize, Deserializer};
use near_sdk::{Promise, PromiseError, assert_one_yocto, ext_contract, serde_json};
use std::str::FromStr;

/// La vista `get_payment` de Arras v1 (`contracts/escrow-v1/src/views.rs`). El trait solo genera `ext_escrow`.
#[allow(dead_code)]
#[ext_contract(ext_escrow)]
pub trait ArrasEscrow {
    fn get_payment(&self, payment_id: PaymentId) -> Option<serde_json::Value>;
}

/// Espejo exacto del `Payment` de Arras v1 (`contracts/escrow-v1/src/lib.rs:82-105`, los 16 campos) tal como lo
/// serializa `get_payment`: falla cerrado (RONDA 1 de c0der, P2-1 (a)). Cualquier otra forma no deserializa y el
/// callback rechaza con `E21` y el deposito entero:
/// - un campo de mas (`deny_unknown_fields`) o de menos (ninguno es opcional), o una clave repetida;
/// - un tipo JSON distinto del de v1: `U128`/`U64` son strings decimales canonicos (solo digitos, sin signo ni ceros a
///   la izquierda salvo `"0"`, dentro del rango: [`canonical`]), `u16` y `bool` son numero y booleano, las cuentas
///   son `AccountId` validos;
/// - un `status` que no es uno de los 5 strings de v1 ([`EscrowStatus`]);
/// - un `operator_id` fuera del charset de v1, `[a-z0-9_-]{1,32}` ([`operator`]).
///
/// Lo que el registro guarda sale de aca (`operator_id`, `released`, las partes); `terms_hash` y `fee_recipient` se
/// exigen con su tipo pero no se validan por contenido (el registro no los usa).
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

/// Los 5 estados de `PaymentStatus` de v1 (`contracts/escrow-v1/src/lib.rs:64-69`), solo como el string que serializa
/// v1 (`"Released"`, nunca `{"Released":null}`). El estado no decide nada (decide `released`): se exige para fallar
/// cerrado ante una respuesta que no es de v1.
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

/// Un `U128`/`U64` de v1: string decimal canonico y dentro del rango del tipo.
fn canonical<'de, D: Deserializer<'de>, T: FromStr>(d: D) -> Result<T, D::Error> {
    let s = String::deserialize(d)?;
    if !is_canonical_uint(&s) {
        return Err(D::Error::custom("numero no canonico"));
    }
    s.parse()
        .map_err(|_| D::Error::custom("numero fuera de rango"))
}

/// El `operator_id` de v1: `[a-z0-9_-]{1,32}` (el mismo `is_valid_operator_id` con el que v1 los da de alta).
fn operator<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let s = String::deserialize(d)?;
    if !is_valid_operator_id(&s) {
        return Err(D::Error::custom("operator_id fuera del charset de v1"));
    }
    Ok(s)
}

impl EscrowPayment {
    /// Hubo plata que paso: `released > 0`, cualquiera sea el estado (RESPUESTA de c0der del 2026-11-03 23:30Z: un
    /// `Refunded` o `Reclaimed` despues de una liberacion parcial tambien es un trato real).
    pub fn has_released(&self) -> bool {
        self.released > 0
    }
}

/// Los argumentos del callback: la calificacion pedida, el deposito y el `nonce` del candado.
#[near(serializers = [json])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingFeedback {
    pub rater: AccountId,
    pub escrow: AccountId,
    pub payment_id: PaymentId,
    pub score: u8,
    pub tag1: Option<String>,
    pub tag2: Option<String>,
    pub feedback_uri: Option<String>,
    /// Ya validado en `give_feedback`.
    pub feedback_hash: Option<String>,
    pub deposit: U128,
    pub nonce: U64,
}

/// Lo que el callback verifico.
pub(crate) struct Checked {
    pub ratee: AccountId,
    pub role: Role,
    pub operator_id: String,
    pub released: u128,
}

#[near]
impl Registry {
    /// Califica a la otra parte de un pago de Arras liberado. Adjuntar `feedback_deposit` (de `get_config`) y al menos
    /// `MIN_GAS_GIVE_FEEDBACK`. Devuelve el `feedback_id` (o `null` si el callback rechaza; el motivo queda en el log
    /// `arras-registry:rechazo:<CODIGO>`).
    #[payable]
    #[allow(clippy::too_many_arguments)]
    pub fn give_feedback(
        &mut self,
        escrow: AccountId,
        payment_id: PaymentId,
        score: u8,
        tag1: Option<String>,
        tag2: Option<String>,
        feedback_uri: Option<String>,
        feedback_hash: Option<String>,
    ) -> Promise {
        if !self.escrows.contains(&escrow) {
            RegistryError::E10EscrowNoPermitido.panic();
        }
        check_payment_id(&payment_id).unwrap_or_else(|e| e.panic());
        if score > MAX_SCORE {
            RegistryError::E12ScoreInvalido.panic();
        }
        check_tag(tag1.as_deref()).unwrap_or_else(|e| e.panic());
        check_tag(tag2.as_deref()).unwrap_or_else(|e| e.panic());
        check_feedback_uri(feedback_uri.as_deref()).unwrap_or_else(|e| e.panic());
        if let Some(h) = &feedback_hash {
            parse_hash(h).unwrap_or_else(|e| e.panic());
        }
        let deposit = env::attached_deposit().as_yoctonear();
        let need = Self::feedback_deposit();
        if deposit < need {
            RegistryError::E16DepositoFeedback(need).panic();
        }
        if env::prepaid_gas() < MIN_GAS_GIVE_FEEDBACK {
            RegistryError::E17GasInsuficiente.panic();
        }
        let rater = env::predecessor_account_id();
        let key = feedback_key(&escrow, &payment_id, &rater);
        if self.feedback_by_key.contains_key(&key) {
            RegistryError::E18YaCalificado.panic();
        }
        let height = env::block_height();
        if let Some(&at) = self.pending.get(&key)
            && height < at.saturating_add(PENDING_TTL_BLOCKS)
        {
            RegistryError::E19EnVuelo.panic();
        }
        self.pending.insert(key, height);
        let pending = PendingFeedback {
            rater,
            escrow: escrow.clone(),
            payment_id: payment_id.clone(),
            score,
            tag1,
            tag2,
            feedback_uri,
            feedback_hash,
            deposit: U128(deposit),
            nonce: U64(height),
        };
        ext_escrow::ext(escrow)
            .with_static_gas(GAS_GET_PAYMENT)
            .get_payment(payment_id)
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(GAS_ON_PAYMENT_CHECKED)
                    .on_payment_checked(pending),
            )
    }

    /// Callback de `give_feedback`. Lee primero el resultado de `get_payment`: fuera de un callback de verdad (sin
    /// resultado) el host aborta antes de tocar el estado o mandar plata.
    #[private]
    pub fn on_payment_checked(&mut self, p: PendingFeedback) -> Option<u32> {
        let result = env::promise_result_checked(0, MAX_PAYMENT_RESULT_LEN);
        let key = feedback_key(&p.escrow, &p.payment_id, &p.rater);
        let outcome = if self.pending.get(&key) == Some(&p.nonce.0) {
            self.pending.remove(&key);
            self.check_payment(&p, &key, result)
        } else {
            Err(RegistryError::E26Reemplazado)
        };
        match outcome {
            Ok(c) => Some(self.accept(p, key, c)),
            Err(e) => {
                env::log_str(&e.rejection_log());
                refund(&p.rater, p.deposit.0);
                None
            }
        }
    }

    /// El rater marca su calificacion como revocada (no se borra; deja de contar en el resumen). 1 yocto. `E30` ->
    /// `E31`.
    #[payable]
    pub fn revoke_feedback(&mut self, escrow: AccountId, payment_id: PaymentId) -> FeedbackView {
        assert_one_yocto();
        let rater = env::predecessor_account_id();
        let key = feedback_key(&escrow, &payment_id, &rater);
        let Some(&id) = self.feedback_by_key.get(&key) else {
            RegistryError::E30FeedbackInexistente.panic();
        };
        let fb = self.feedbacks.get_mut(id).unwrap_or_else(|| env::abort());
        if fb.revoked {
            RegistryError::E31YaRevocado.panic();
        }
        fb.revoked = true;
        fb.revoked_at = env::block_timestamp();
        let fb = fb.clone();
        if let Some(s) = self.summaries.get_mut(&fb.ratee) {
            s.revoke(fb.ratee_role, fb.score);
        }
        if let Some(s) = self
            .summaries_by_operator
            .get_mut(&(fb.ratee.clone(), fb.operator_id.clone()))
        {
            s.revoke(fb.ratee_role, fb.score);
        }
        RegistryEvent::FeedbackRevoked(vec![FeedbackRevokedData {
            feedback_id: id,
            rater: fb.rater.clone(),
            ratee: fb.ratee.clone(),
            escrow: fb.escrow.clone(),
            payment_id: fb.payment_id.clone(),
            revoked_at: U64(fb.revoked_at),
        }])
        .emit();
        FeedbackView::of(id, &fb)
    }
}

impl Registry {
    /// Las guardas del callback sobre la respuesta de `get_payment`, sin escribir nada.
    pub(crate) fn check_payment(
        &self,
        p: &PendingFeedback,
        key: &[u8; 32],
        result: Result<Vec<u8>, PromiseError>,
    ) -> Result<Checked, RegistryError> {
        let bytes = match result {
            Ok(b) => b,
            Err(PromiseError::Failed) => return Err(RegistryError::E20GetPaymentFallo),
            Err(_) => return Err(RegistryError::E21RespuestaInvalida),
        };
        let payment: Option<EscrowPayment> =
            serde_json::from_slice(&bytes).map_err(|_| RegistryError::E21RespuestaInvalida)?;
        let pay = payment.ok_or(RegistryError::E22PagoInexistente)?;
        if !pay.has_released() {
            return Err(RegistryError::E23PagoSinLiberar);
        }
        let (ratee, role) = if p.rater == pay.payer {
            (pay.receiver, Role::Receiver)
        } else if p.rater == pay.receiver {
            (pay.payer, Role::Payer)
        } else {
            return Err(RegistryError::E24NoEsParte);
        };
        if ratee == p.rater {
            return Err(RegistryError::E25AutoCalificacion);
        }
        if self.feedback_by_key.contains_key(key) {
            return Err(RegistryError::E18YaCalificado);
        }
        Ok(Checked {
            ratee,
            role,
            operator_id: pay.operator_id,
            released: pay.released,
        })
    }

    /// Escribe la calificacion, cobra el storage medido (con tope en el deposito) y devuelve el resto.
    fn accept(&mut self, p: PendingFeedback, key: [u8; 32], c: Checked) -> u32 {
        let now = env::block_timestamp();
        let fb = Feedback {
            rater: p.rater,
            ratee: c.ratee,
            ratee_role: c.role,
            escrow: p.escrow,
            payment_id: p.payment_id,
            operator_id: c.operator_id,
            released: c.released,
            score: p.score,
            tag1: p.tag1,
            tag2: p.tag2,
            feedback_uri: p.feedback_uri,
            feedback_hash: p.feedback_hash.as_deref().and_then(|h| parse_hash(h).ok()),
            created_at: now,
            revoked: false,
            revoked_at: 0,
        };
        self.flush_all();
        let before = env::storage_usage();
        let id = self.feedbacks.len();
        self.feedbacks.push(fb.clone());
        self.feedback_by_key.insert(key, id);
        let index = self
            .received
            .entry(fb.ratee.clone())
            .or_insert_with(|| Vector::new(received_prefix(&fb.ratee)));
        index.push(id);
        index.flush();
        self.summaries
            .entry(fb.ratee.clone())
            .or_default()
            .add(fb.ratee_role, fb.score, id, now);
        self.summaries_by_operator
            .entry((fb.ratee.clone(), fb.operator_id.clone()))
            .or_default()
            .add(fb.ratee_role, fb.score, id, now);
        self.flush_all();
        let after = env::storage_usage();
        let cost = bytes_cost(after.saturating_sub(before)).min(p.deposit.0);
        refund(&fb.rater, p.deposit.0 - cost);
        let v = FeedbackView::of(id, &fb);
        RegistryEvent::FeedbackGiven(vec![FeedbackGivenData {
            feedback_id: id,
            rater: v.rater,
            ratee: v.ratee,
            ratee_role: v.ratee_role,
            escrow: v.escrow,
            payment_id: v.payment_id,
            operator_id: v.operator_id,
            released: v.released,
            score: v.score,
            tag1: v.tag1,
            tag2: v.tag2,
            feedback_uri: v.feedback_uri,
            feedback_hash: v.feedback_hash,
            created_at: v.created_at,
        }])
        .emit();
        id
    }
}
