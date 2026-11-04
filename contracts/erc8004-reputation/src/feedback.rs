//! El nucleo: `give_feedback` (con la llamada cruzada a `check_agent`), su callback `on_feedback_checked`,
//! `revoke_feedback` y `append_response`.
//!
//! `give_feedback`, en orden (cada una entra en panico y el runtime devuelve el deposito): `E01` agent_id -> `E02`
//! value -> `E04` value_decimals -> `E03` value fuera de `+-1e38` -> `E05` tags -> `E06` endpoint -> `E07`
//! feedback_uri -> `E08` feedback_hash -> `E09` deposito -> `E10` gas.
//!
//! `on_feedback_checked` (nunca entra en panico): `E20` la vista fallo -> `E21` la respuesta no es exactamente un
//! `AgentCheck` ni `null` -> `E22` `null` (el agente no existe) -> `E23` el cliente es el dueno o un aprobado. Un
//! rechazo no escribe nada y devuelve todo lo adjuntado; una aceptada cobra el storage medido y devuelve el resto.

use crate::events::{FeedbackRevokedData, NewFeedbackData, ResponseAppendedData};
use crate::*;
use erc8004_common::events::emit;
use erc8004_common::storage::{bytes_cost, refund};
use erc8004_common::types::{AgentCheck, MAX_AGENT_CHECK_LEN, parse_agent_check};
use erc8004_common::validate::{
    MAX_ABS_VALUE, MAX_VALUE_DECIMALS, is_valid_optional_uri, is_valid_tag, is_valid_uri,
    parse_hash, parse_value,
};
use near_sdk::json_types::U128;
use near_sdk::{Promise, PromiseError, assert_one_yocto, ext_contract};

/// La vista del Identity Registry. El trait solo genera `ext_identity`.
#[allow(dead_code)]
#[ext_contract(ext_identity)]
pub trait IdentityRegistry {
    fn check_agent(&self, agent_id: String, account_id: AccountId) -> Option<AgentCheck>;
}

/// Los argumentos del callback: la calificacion pedida (ya validada) y el deposito.
#[near(serializers = [json])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingFeedback {
    pub client: AccountId,
    /// Canonico.
    pub agent_id: String,
    /// Canonico, dentro de `+-1e38`.
    pub value: String,
    pub value_decimals: u8,
    pub tag1: String,
    pub tag2: String,
    pub endpoint: String,
    pub feedback_uri: String,
    /// 64 hex en minusculas o `null`.
    pub feedback_hash: Option<String>,
    pub deposit: U128,
}

impl ReputationRegistry {
    /// Las guardas sincronicas de `give_feedback` y `give_feedback_with_proof` (sin el deposito ni el gas).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn validate_feedback(
        agent_id: String,
        value: String,
        value_decimals: u8,
        tag1: Option<String>,
        tag2: Option<String>,
        endpoint: Option<String>,
        feedback_uri: Option<String>,
        feedback_hash: Option<String>,
    ) -> PendingFeedback {
        parse_agent_id(&agent_id);
        let v = parse_value(&value).unwrap_or_else(|| ReputationError::E02ValueInvalido.panic());
        if value_decimals > MAX_VALUE_DECIMALS {
            ReputationError::E04DemasiadosDecimales.panic();
        }
        if v.unsigned_abs() > MAX_ABS_VALUE {
            ReputationError::E03ValueFueraDeRango.panic();
        }
        let tag1 = tag1.unwrap_or_default();
        let tag2 = tag2.unwrap_or_default();
        if !is_valid_tag(&tag1) || !is_valid_tag(&tag2) {
            ReputationError::E05TagInvalido.panic();
        }
        let endpoint = endpoint.unwrap_or_default();
        if !is_valid_optional_uri(&endpoint, MAX_ENDPOINT_LEN) {
            ReputationError::E06EndpointInvalido.panic();
        }
        let feedback_uri = feedback_uri.unwrap_or_default();
        if !is_valid_optional_uri(&feedback_uri, MAX_FEEDBACK_URI_LEN) {
            ReputationError::E07FeedbackUriInvalida.panic();
        }
        if let Some(h) = &feedback_hash
            && parse_hash(h).is_none()
        {
            ReputationError::E08HashInvalido.panic();
        }
        PendingFeedback {
            client: env::predecessor_account_id(),
            agent_id,
            value,
            value_decimals,
            tag1,
            tag2,
            endpoint,
            feedback_uri,
            feedback_hash,
            deposit: U128(env::attached_deposit().as_yoctonear()),
        }
    }

    /// `E09` si el deposito no alcanza para `need`, `E10` si el gas no llega a `min_gas`.
    pub(crate) fn require_deposit_and_gas(need: u128, min_gas: Gas) {
        if env::attached_deposit().as_yoctonear() < need {
            ReputationError::E09DepositoInsuficiente(need).panic();
        }
        if env::prepaid_gas() < min_gas {
            ReputationError::E10GasInsuficiente.panic();
        }
    }

    /// Lee la respuesta de `check_agent`: el agente existe y el cliente no es su dueno ni un aprobado.
    pub(crate) fn check_identity(
        result: Result<Vec<u8>, PromiseError>,
    ) -> Result<AgentCheck, ReputationError> {
        let bytes = match result {
            Ok(b) => b,
            Err(PromiseError::Failed) => return Err(ReputationError::E20VistaDeIdentidadFallo),
            Err(_) => return Err(ReputationError::E21RespuestaDeIdentidadInvalida),
        };
        let check = parse_agent_check(&bytes)
            .map_err(|_| ReputationError::E21RespuestaDeIdentidadInvalida)?
            .ok_or(ReputationError::E22AgenteInexistente)?;
        if check.authorized {
            return Err(ReputationError::E23AutoCalificacion);
        }
        Ok(check)
    }

    /// Un rechazo de un callback: log, todo el deposito de vuelta, `null`.
    pub(crate) fn reject(p: &PendingFeedback, e: ReputationError) -> Option<String> {
        env::log_str(&e.rejection_log());
        refund(&p.client, p.deposit.0);
        None
    }

    /// Escribe la calificacion (y la prueba, si viene), cobra el storage medido (con tope en el deposito), devuelve el
    /// resto y emite `new_feedback`. Devuelve el `feedback_index`.
    pub(crate) fn accept(&mut self, p: PendingFeedback, proof: Option<ProofToStore>) -> String {
        let id = parse_agent_id(&p.agent_id);
        let value = parse_value(&p.value).unwrap_or_else(|| env::abort());
        let before = self.usage();
        let index = self
            .last(id, &p.client)
            .checked_add(1)
            .unwrap_or_else(|| env::abort());
        self.last_index.insert((id, p.client.clone()), index);
        self.feedback.insert(
            (id, p.client.clone(), index),
            Feedback {
                value,
                value_decimals: p.value_decimals,
                is_revoked: false,
                tag1: p.tag1.clone(),
                tag2: p.tag2.clone(),
            },
        );
        if index == 1 {
            let list = self
                .clients
                .entry(id)
                .or_insert_with(|| Vector::new(clients_prefix(id)));
            list.push(p.client.clone());
            list.flush();
        }
        let proof_data = proof.map(|pr| self.store_proof(id, &p.client, index, pr));
        let after = self.usage();
        let cost = bytes_cost(after.saturating_sub(before)).min(p.deposit.0);
        refund(&p.client, p.deposit.0 - cost);
        emit(
            "new_feedback",
            &NewFeedbackData {
                agent_id: p.agent_id,
                client_address: p.client,
                feedback_index: index.to_string(),
                value: p.value,
                value_decimals: p.value_decimals,
                tag1: p.tag1,
                tag2: p.tag2,
                endpoint: p.endpoint,
                feedback_uri: p.feedback_uri,
                feedback_hash: p.feedback_hash,
                proof: proof_data,
            },
        );
        index.to_string()
    }
}

/// Lo que `accept` guarda de una prueba (sin el feature, nunca hay).
#[cfg(feature = "feedback-proofs")]
pub(crate) type ProofToStore = crate::proof::ProofToStore;
#[cfg(not(feature = "feedback-proofs"))]
pub(crate) type ProofToStore = std::convert::Infallible;

#[cfg(not(feature = "feedback-proofs"))]
impl ReputationRegistry {
    fn store_proof(
        &mut self,
        _id: u64,
        _client: &AccountId,
        _index: u64,
        p: ProofToStore,
    ) -> crate::events::ProofData {
        match p {}
    }
}

#[near]
impl ReputationRegistry {
    /// `giveFeedback`. `value`: entero con signo como string (`"-32"`), `|value| <= 1e38`; `value_decimals` 0..=18;
    /// tags de hasta 64 bytes; `endpoint` y `feedback_uri` URIs (o ausentes); `feedback_hash` 64 hex. Adjuntar
    /// `feedback_deposit` y al menos `min_gas_give_feedback` (`get_config`). Devuelve el `feedback_index` (o `null` si
    /// el callback rechaza; el motivo queda en el log `erc8004:rechazo:<CODIGO>`).
    #[payable]
    #[allow(clippy::too_many_arguments)]
    pub fn give_feedback(
        &mut self,
        agent_id: String,
        value: String,
        value_decimals: u8,
        tag1: Option<String>,
        tag2: Option<String>,
        endpoint: Option<String>,
        feedback_uri: Option<String>,
        feedback_hash: Option<String>,
    ) -> Promise {
        let p = Self::validate_feedback(
            agent_id,
            value,
            value_decimals,
            tag1,
            tag2,
            endpoint,
            feedback_uri,
            feedback_hash,
        );
        Self::require_deposit_and_gas(Self::feedback_deposit(), MIN_GAS_GIVE_FEEDBACK);
        ext_identity::ext(self.identity_registry.clone())
            .with_static_gas(GAS_CHECK_AGENT)
            .check_agent(p.agent_id.clone(), p.client.clone())
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(GAS_ON_FEEDBACK_CHECKED)
                    .on_feedback_checked(p),
            )
    }

    /// Callback de `give_feedback`. Lee primero el resultado: fuera de un callback de verdad (sin resultado) el host
    /// aborta antes de tocar el estado o mandar plata.
    #[private]
    pub fn on_feedback_checked(&mut self, p: PendingFeedback) -> Option<String> {
        let result = env::promise_result_checked(0, MAX_AGENT_CHECK_LEN);
        match Self::check_identity(result) {
            Ok(_) => Some(self.accept(p, None)),
            Err(e) => Self::reject(&p, e),
        }
    }

    /// `revokeFeedback`: el cliente marca su calificacion (no se borra; deja de contar). Exactamente 1 yocto.
    #[payable]
    pub fn revoke_feedback(&mut self, agent_id: String, feedback_index: String) {
        assert_one_yocto();
        let id = parse_agent_id(&agent_id);
        let index = parse_index(&feedback_index);
        let client = env::predecessor_account_id();
        if index == 0 {
            ReputationError::E12IndexCero.panic();
        }
        if index > self.last(id, &client) {
            ReputationError::E13IndexFueraDeRango.panic();
        }
        let fb = self
            .feedback
            .get_mut(&(id, client.clone(), index))
            .unwrap_or_else(|| env::abort());
        if fb.is_revoked {
            ReputationError::E14YaRevocada.panic();
        }
        fb.is_revoked = true;
        emit(
            "feedback_revoked",
            &FeedbackRevokedData {
                agent_id: id.to_string(),
                client_address: client,
                feedback_index: index.to_string(),
            },
        );
    }

    /// `appendResponse`: **cualquiera** responde a una calificacion (tambien revocada). `response_uri` no vacia.
    /// Al menos 1 yocto y lo que crezca (un respondedor nuevo); vuelve el sobrante.
    #[payable]
    pub fn append_response(
        &mut self,
        agent_id: String,
        client_address: AccountId,
        feedback_index: String,
        response_uri: String,
        response_hash: Option<String>,
    ) {
        let deposit = env::attached_deposit().as_yoctonear();
        if deposit == 0 {
            ReputationError::E15RequiereDeposito.panic();
        }
        let id = parse_agent_id(&agent_id);
        let index = parse_index(&feedback_index);
        if index == 0 {
            ReputationError::E12IndexCero.panic();
        }
        if !is_valid_uri(&response_uri, MAX_RESPONSE_URI_LEN) {
            ReputationError::E16ResponseUriInvalida.panic();
        }
        if index > self.last(id, &client_address) {
            ReputationError::E13IndexFueraDeRango.panic();
        }
        if let Some(h) = &response_hash
            && parse_hash(h).is_none()
        {
            ReputationError::E08HashInvalido.panic();
        }
        let responder = env::predecessor_account_id();
        let key: FeedbackKey = (id, client_address.clone(), index);
        let before = self.usage();
        let count_key = (id, client_address.clone(), index, responder.clone());
        let n = self.response_count.get(&count_key).copied().unwrap_or(0);
        if n == 0 {
            let list = self
                .responders
                .entry(key.clone())
                .or_insert_with(|| Vector::new(responders_prefix(&key)));
            list.push(responder.clone());
            list.flush();
        }
        self.response_count
            .insert(count_key, n.checked_add(1).unwrap_or_else(|| env::abort()));
        let after = self.usage();
        let cost = bytes_cost(after.saturating_sub(before));
        if deposit < cost {
            ReputationError::E09DepositoInsuficiente(cost).panic();
        }
        refund(&responder, deposit - cost);
        emit(
            "response_appended",
            &ResponseAppendedData {
                agent_id: id.to_string(),
                client_address,
                feedback_index: index.to_string(),
                responder,
                response_uri,
                response_hash,
            },
        );
    }
}

impl ReputationRegistry {
    /// Lo que `give_feedback` exige adjuntar: el peor caso de una calificacion.
    pub fn feedback_deposit() -> u128 {
        bytes_cost(FEEDBACK_MAX_BYTES)
    }

    /// Lo que `give_feedback_with_proof` exige adjuntar.
    pub fn feedback_proof_deposit() -> u128 {
        bytes_cost(FEEDBACK_PROOF_MAX_BYTES)
    }
}
