//! **BORRADOR, NO DESPLEGABLE.** ERC-8004 Validation Registry en NEAR (`docs/plans/erc8004-near-port.md`, seccion 4),
//! port de `ValidationRegistryUpgradeable` 2.0.0 (`erc-8004/erc-8004-contracts` b9e466c). El equipo de ERC-8004 dice
//! que esa seccion "is still undergoing technical due diligence and is not yet available" (8004.org/learn, CONTEXTO 1
//! de c0der): este crate queda como diseno probado y no entra al plan de deploy. `get_config.status` = `"BORRADOR"`.
//!
//! - `validation_request(validator_address, agent_id, request_uri, request_hash)`: solo el dueno o un aprobado del
//!   agente (llamada cruzada a `check_agent` con callback que falla cerrado). `request_hash` es unico global, con
//!   candado mientras vuela.
//! - `validation_response(request_hash, response, response_uri?, response_hash?, tag?)`: solo el validador nombrado,
//!   `response` 0..=100, se puede llamar muchas veces.
//! - Vistas `get_validation_status`, `get_summary`, `get_agent_validations`, `get_validator_requests`.

use erc8004_common::events::emit;
use erc8004_common::storage::{ACCOUNT_MAX_BYTES, bytes_cost, record_bytes, refund, string_bytes};
use erc8004_common::types::{AgentCheck, MAX_AGENT_CHECK_LEN, parse_agent_check};
use erc8004_common::validate::{hex, is_valid_tag, is_valid_uri, parse_hash, parse_u64};
use near_sdk::json_types::{U64, U128};
use near_sdk::serde::Serialize;
use near_sdk::store::{LookupMap, Vector};
use near_sdk::{AccountId, Gas, PanicOnDefault, Promise, PromiseError, env, ext_contract, near};

#[cfg(test)]
mod unit_tests;

pub const MAX_REQUEST_URI_LEN: usize = 4096;
pub const MAX_RESPONSE_URI_LEN: usize = 4096;
pub const MAX_RESPONSE: u8 = 100;
pub const MAX_LIST_LIMIT: u64 = 100;
pub const GAS_CHECK_AGENT: Gas = Gas::from_tgas(5);
pub const GAS_ON_REQUEST_CHECKED: Gas = Gas::from_tgas(20);
pub const MIN_GAS_VALIDATION_REQUEST: Gas = Gas::from_tgas(40);
pub const PENDING_TTL_BLOCKS: u64 = 1_000;

pub mod prefix {
    pub const VALIDATIONS: &[u8] = b"s";
    pub const AGENT_VALIDATIONS: &[u8] = b"a";
    pub const AGENT_LIST: u8 = b'A';
    pub const VALIDATOR_REQUESTS: &[u8] = b"v";
    pub const VALIDATOR_LIST: u8 = b'V';
    pub const PENDING: &[u8] = b"k";
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationError {
    E01AgentIdInvalido,
    E02RequestUriInvalida,
    E03HashInvalido,
    E04Existe,
    E05EnVuelo,
    E06ValidadorInvalido,
    E07DepositoInsuficiente(u128),
    E08GasInsuficiente,
    E09RequiereDeposito,
    E10Desconocida,
    E11NoEsElValidador,
    E12RespuestaFueraDeRango,
    E13ResponseUriInvalida,
    E14TagInvalido,
    E15IdentityRegistryInvalido,
    E20VistaDeIdentidadFallo,
    E21RespuestaDeIdentidadInvalida,
    E22AgenteInexistente,
    E23NoAutorizado,
    E30Reemplazado,
}

impl ValidationError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::E01AgentIdInvalido => "E01_AGENT_ID_INVALIDO",
            Self::E02RequestUriInvalida => "E02_REQUEST_URI_INVALIDA",
            Self::E03HashInvalido => "E03_HASH_INVALIDO",
            Self::E04Existe => "E04_EXISTE",
            Self::E05EnVuelo => "E05_EN_VUELO",
            Self::E06ValidadorInvalido => "E06_VALIDADOR_INVALIDO",
            Self::E07DepositoInsuficiente(_) => "E07_DEPOSITO_INSUFICIENTE",
            Self::E08GasInsuficiente => "E08_GAS_INSUFICIENTE",
            Self::E09RequiereDeposito => "E09_REQUIERE_DEPOSITO",
            Self::E10Desconocida => "E10_DESCONOCIDA",
            Self::E11NoEsElValidador => "E11_NO_ES_EL_VALIDADOR",
            Self::E12RespuestaFueraDeRango => "E12_RESPUESTA_FUERA_DE_RANGO",
            Self::E13ResponseUriInvalida => "E13_RESPONSE_URI_INVALIDA",
            Self::E14TagInvalido => "E14_TAG_INVALIDO",
            Self::E15IdentityRegistryInvalido => "E15_IDENTITY_REGISTRY_INVALIDO",
            Self::E20VistaDeIdentidadFallo => "E20_VISTA_DE_IDENTIDAD_FALLO",
            Self::E21RespuestaDeIdentidadInvalida => "E21_RESPUESTA_DE_IDENTIDAD_INVALIDA",
            Self::E22AgenteInexistente => "E22_AGENTE_INEXISTENTE",
            Self::E23NoAutorizado => "E23_NO_AUTORIZADO",
            Self::E30Reemplazado => "E30_REEMPLAZADO",
        }
    }

    pub fn message(&self) -> String {
        match self {
            Self::E01AgentIdInvalido => "agent_id invalido".into(),
            Self::E02RequestUriInvalida => "request_uri invalida".into(),
            Self::E03HashInvalido => "hash invalido: 64 hex en minusculas, sin 0x".into(),
            Self::E04Existe => "exists".into(),
            Self::E05EnVuelo => "hay un validation_request con este hash en vuelo".into(),
            Self::E06ValidadorInvalido => "bad validator".into(),
            Self::E07DepositoInsuficiente(n) => {
                format!("deposito insuficiente para el storage: se necesitan {n} yocto")
            }
            Self::E08GasInsuficiente => "gas insuficiente para la llamada cruzada".into(),
            Self::E09RequiereDeposito => "requiere al menos 1 yocto adjunto".into(),
            Self::E10Desconocida => "unknown".into(),
            Self::E11NoEsElValidador => "not validator".into(),
            Self::E12RespuestaFueraDeRango => "resp>100".into(),
            Self::E13ResponseUriInvalida => "response_uri invalida".into(),
            Self::E14TagInvalido => "tag invalido".into(),
            Self::E15IdentityRegistryInvalido => "identity_registry invalido".into(),
            Self::E20VistaDeIdentidadFallo => "check_agent del Identity Registry fallo".into(),
            Self::E21RespuestaDeIdentidadInvalida => "respuesta de check_agent invalida".into(),
            Self::E22AgenteInexistente => "agente inexistente".into(),
            Self::E23NoAutorizado => "Not authorized".into(),
            Self::E30Reemplazado => "otro pedido con el mismo hash reemplazo a este".into(),
        }
    }

    pub fn panic(&self) -> ! {
        env::panic_str(&format!(
            "ERC8004_VAL_{}: {}",
            &self.code()[..3],
            self.message()
        ))
    }

    pub fn rejection_log(&self) -> String {
        format!("erc8004:rechazo:{}", self.code())
    }
}

#[allow(dead_code)]
#[ext_contract(ext_identity)]
pub trait IdentityRegistry {
    fn check_agent(&self, agent_id: String, account_id: AccountId) -> Option<AgentCheck>;
}

/// `ValidationStatus` de EVM. `response_hash` y `tag` los pone el validador (el pedido los deja en `None` y `""`).
#[near(serializers = [borsh])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Validation {
    pub validator: AccountId,
    pub agent_id: u64,
    pub response: u8,
    pub response_hash: Option<[u8; 32]>,
    pub tag: String,
    pub last_update: u64,
    pub has_response: bool,
}

/// Peor caso de lo que escribe un `validation_request` aceptado: el `Validation` con un validador de 64, y los dos
/// `Vector` nuevos (agente y validador) con su elemento.
pub const REQUEST_MAX_BYTES: u64 = {
    let validation = record_bytes(
        1 + 32,
        ACCOUNT_MAX_BYTES + 8 + 1 + 1 + string_bytes(0) + 8 + 1,
    );
    let agent_list = record_bytes(1 + 8, 4 + 4 + 9);
    let agent_elem = record_bytes(9 + 4, 32);
    let validator_list = record_bytes(1 + ACCOUNT_MAX_BYTES, 4 + 4 + 33);
    let validator_elem = record_bytes(33 + 4, 32);
    validation + agent_list + agent_elem + validator_list + validator_elem
};

#[near(serializers = [json])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingRequest {
    pub requester: AccountId,
    pub validator_address: AccountId,
    pub agent_id: String,
    pub request_uri: String,
    pub request_hash: String,
    pub deposit: U128,
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(crate = "near_sdk::serde")]
pub struct ValidationRequestData {
    pub validator_address: AccountId,
    pub agent_id: String,
    pub request_uri: String,
    pub request_hash: String,
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(crate = "near_sdk::serde")]
pub struct ValidationResponseData {
    pub validator_address: AccountId,
    pub agent_id: String,
    pub request_hash: String,
    pub response: u8,
    pub response_uri: String,
    pub response_hash: Option<String>,
    pub tag: String,
}

#[near(serializers = [json])]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationStatusView {
    pub validator_address: AccountId,
    pub agent_id: String,
    pub response: u8,
    pub response_hash: Option<String>,
    pub tag: String,
    pub last_update: U64,
    pub has_response: bool,
}

#[near(serializers = [json])]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationSummaryView {
    pub count: U64,
    pub average_response: u8,
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct ValidationRegistry {
    identity_registry: AccountId,
    validations: LookupMap<[u8; 32], Validation>,
    agent_validations: LookupMap<u64, Vector<[u8; 32]>>,
    validator_requests: LookupMap<AccountId, Vector<[u8; 32]>>,
    pending: LookupMap<[u8; 32], u64>,
}

fn agent_prefix(id: u64) -> Vec<u8> {
    let mut p = vec![prefix::AGENT_LIST];
    p.extend_from_slice(&id.to_le_bytes());
    p
}

fn validator_prefix(v: &AccountId) -> Vec<u8> {
    let mut p = vec![prefix::VALIDATOR_LIST];
    p.extend_from_slice(&env::sha256_array(v.as_bytes()));
    p
}

fn page(list: &Vector<[u8; 32]>, from: Option<u64>, limit: Option<u64>) -> Vec<String> {
    let from = from.unwrap_or(0);
    let limit = limit.unwrap_or(MAX_LIST_LIMIT).min(MAX_LIST_LIMIT);
    let end = u64::from(list.len()).min(from.saturating_add(limit));
    (from..end)
        .filter_map(|i| list.get(i as u32))
        .map(hex)
        .collect()
}

impl ValidationRegistry {
    fn usage(&mut self) -> u64 {
        self.validations.flush();
        self.agent_validations.flush();
        self.validator_requests.flush();
        self.pending.flush();
        env::storage_usage()
    }

    fn check(
        &self,
        hash: &[u8; 32],
        result: Result<Vec<u8>, PromiseError>,
    ) -> Result<(), ValidationError> {
        let bytes = match result {
            Ok(b) => b,
            Err(PromiseError::Failed) => return Err(ValidationError::E20VistaDeIdentidadFallo),
            Err(_) => return Err(ValidationError::E21RespuestaDeIdentidadInvalida),
        };
        let c = parse_agent_check(&bytes)
            .map_err(|_| ValidationError::E21RespuestaDeIdentidadInvalida)?
            .ok_or(ValidationError::E22AgenteInexistente)?;
        if !c.authorized {
            return Err(ValidationError::E23NoAutorizado);
        }
        if self.validations.contains_key(hash) {
            return Err(ValidationError::E04Existe);
        }
        Ok(())
    }
}

#[near]
impl ValidationRegistry {
    #[init]
    #[private]
    pub fn new(identity_registry: AccountId) -> Self {
        if identity_registry == env::current_account_id() {
            ValidationError::E15IdentityRegistryInvalido.panic();
        }
        Self {
            identity_registry,
            validations: LookupMap::new(prefix::VALIDATIONS),
            agent_validations: LookupMap::new(prefix::AGENT_VALIDATIONS),
            validator_requests: LookupMap::new(prefix::VALIDATOR_REQUESTS),
            pending: LookupMap::new(prefix::PENDING),
        }
    }

    /// `validationRequest`: el dueno o un aprobado del agente. Adjuntar `request_deposit`; vuelve el sobrante (todo
    /// si se rechaza). Devuelve `true` si quedo, `false` si el callback rechazo (log `erc8004:rechazo:<CODIGO>`).
    #[payable]
    pub fn validation_request(
        &mut self,
        validator_address: AccountId,
        agent_id: String,
        request_uri: String,
        request_hash: String,
    ) -> Promise {
        if parse_u64(&agent_id).is_none() {
            ValidationError::E01AgentIdInvalido.panic();
        }
        if !is_valid_uri(&request_uri, MAX_REQUEST_URI_LEN) {
            ValidationError::E02RequestUriInvalida.panic();
        }
        let hash =
            parse_hash(&request_hash).unwrap_or_else(|| ValidationError::E03HashInvalido.panic());
        if validator_address == env::current_account_id() {
            ValidationError::E06ValidadorInvalido.panic();
        }
        if self.validations.contains_key(&hash) {
            ValidationError::E04Existe.panic();
        }
        let need = bytes_cost(REQUEST_MAX_BYTES);
        let deposit = env::attached_deposit().as_yoctonear();
        if deposit < need {
            ValidationError::E07DepositoInsuficiente(need).panic();
        }
        if env::prepaid_gas() < MIN_GAS_VALIDATION_REQUEST {
            ValidationError::E08GasInsuficiente.panic();
        }
        let height = env::block_height();
        if let Some(&at) = self.pending.get(&hash)
            && height < at.saturating_add(PENDING_TTL_BLOCKS)
        {
            ValidationError::E05EnVuelo.panic();
        }
        self.pending.insert(hash, height);
        let requester = env::predecessor_account_id();
        let p = PendingRequest {
            requester: requester.clone(),
            validator_address,
            agent_id: agent_id.clone(),
            request_uri,
            request_hash,
            deposit: U128(deposit),
        };
        ext_identity::ext(self.identity_registry.clone())
            .with_static_gas(GAS_CHECK_AGENT)
            .check_agent(agent_id, requester)
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(GAS_ON_REQUEST_CHECKED)
                    .on_request_checked(p, U64(height)),
            )
    }

    #[private]
    pub fn on_request_checked(&mut self, p: PendingRequest, nonce: U64) -> bool {
        let result = env::promise_result_checked(0, MAX_AGENT_CHECK_LEN);
        let hash = parse_hash(&p.request_hash).unwrap_or_else(|| env::abort());
        let outcome = if self.pending.get(&hash) == Some(&nonce.0) {
            self.pending.remove(&hash);
            self.check(&hash, result)
        } else {
            Err(ValidationError::E30Reemplazado)
        };
        if let Err(e) = outcome {
            env::log_str(&e.rejection_log());
            refund(&p.requester, p.deposit.0);
            return false;
        }
        let id = parse_u64(&p.agent_id).unwrap_or_else(|| env::abort());
        let before = self.usage();
        self.validations.insert(
            hash,
            Validation {
                validator: p.validator_address.clone(),
                agent_id: id,
                response: 0,
                response_hash: None,
                tag: String::new(),
                last_update: env::block_timestamp(),
                has_response: false,
            },
        );
        let a = self
            .agent_validations
            .entry(id)
            .or_insert_with(|| Vector::new(agent_prefix(id)));
        a.push(hash);
        a.flush();
        let v = self
            .validator_requests
            .entry(p.validator_address.clone())
            .or_insert_with(|| Vector::new(validator_prefix(&p.validator_address)));
        v.push(hash);
        v.flush();
        let after = self.usage();
        let cost = bytes_cost(after.saturating_sub(before)).min(p.deposit.0);
        refund(&p.requester, p.deposit.0 - cost);
        emit(
            "validation_request",
            &ValidationRequestData {
                validator_address: p.validator_address,
                agent_id: p.agent_id,
                request_uri: p.request_uri,
                request_hash: p.request_hash,
            },
        );
        true
    }

    /// `validationResponse`: solo el validador nombrado; `response` 0..=100; se puede llamar muchas veces. Al menos 1
    /// yocto y lo que crezca (el `tag` y el `response_hash`); lo liberado vuelve al validador.
    #[payable]
    pub fn validation_response(
        &mut self,
        request_hash: String,
        response: u8,
        response_uri: Option<String>,
        response_hash: Option<String>,
        tag: Option<String>,
    ) {
        let deposit = env::attached_deposit().as_yoctonear();
        if deposit == 0 {
            ValidationError::E09RequiereDeposito.panic();
        }
        let hash =
            parse_hash(&request_hash).unwrap_or_else(|| ValidationError::E03HashInvalido.panic());
        let Some(s) = self.validations.get(&hash).cloned() else {
            ValidationError::E10Desconocida.panic();
        };
        let validator = env::predecessor_account_id();
        if validator != s.validator {
            ValidationError::E11NoEsElValidador.panic();
        }
        if response > MAX_RESPONSE {
            ValidationError::E12RespuestaFueraDeRango.panic();
        }
        let response_uri = response_uri.unwrap_or_default();
        if !response_uri.is_empty() && !is_valid_uri(&response_uri, MAX_RESPONSE_URI_LEN) {
            ValidationError::E13ResponseUriInvalida.panic();
        }
        let rh = response_hash
            .as_ref()
            .map(|h| parse_hash(h).unwrap_or_else(|| ValidationError::E03HashInvalido.panic()));
        let tag = tag.unwrap_or_default();
        if !is_valid_tag(&tag) {
            ValidationError::E14TagInvalido.panic();
        }
        let before = self.usage();
        self.validations.insert(
            hash,
            Validation {
                response,
                response_hash: rh,
                tag: tag.clone(),
                last_update: env::block_timestamp(),
                has_response: true,
                ..s.clone()
            },
        );
        let after = self.usage();
        if after >= before {
            let cost = bytes_cost(after - before);
            if deposit < cost {
                ValidationError::E07DepositoInsuficiente(cost).panic();
            }
            refund(&validator, deposit - cost);
        } else {
            refund(&validator, deposit + bytes_cost(before - after));
        }
        emit(
            "validation_response",
            &ValidationResponseData {
                validator_address: s.validator,
                agent_id: s.agent_id.to_string(),
                request_hash,
                response,
                response_uri,
                response_hash,
                tag,
            },
        );
    }

    /// `getValidationStatus`: `null` si no existe (EVM: `"unknown"`).
    pub fn get_validation_status(&self, request_hash: String) -> Option<ValidationStatusView> {
        let s = self.validations.get(&parse_hash(&request_hash)?)?;
        Some(ValidationStatusView {
            validator_address: s.validator.clone(),
            agent_id: s.agent_id.to_string(),
            response: s.response,
            response_hash: s.response_hash.as_ref().map(hex),
            tag: s.tag.clone(),
            last_update: U64(s.last_update),
            has_response: s.has_response,
        })
    }

    /// `getSummary`: solo las respondidas; sin `validator_addresses` (o vacia) = todos; `tag` ausente o `""` = sin
    /// filtro; `average_response = floor(suma / count)`.
    pub fn get_summary(
        &self,
        agent_id: String,
        validator_addresses: Option<Vec<AccountId>>,
        tag: Option<String>,
    ) -> ValidationSummaryView {
        let id =
            parse_u64(&agent_id).unwrap_or_else(|| ValidationError::E01AgentIdInvalido.panic());
        let validators = validator_addresses.filter(|v| !v.is_empty());
        let tag = tag.filter(|t| !t.is_empty());
        let mut count = 0u64;
        let mut total = 0u64;
        if let Some(list) = self.agent_validations.get(&id) {
            for h in list.iter() {
                let Some(s) = self.validations.get(h) else {
                    continue;
                };
                if !s.has_response
                    || validators
                        .as_ref()
                        .is_some_and(|v| !v.contains(&s.validator))
                    || tag.as_ref().is_some_and(|t| t != &s.tag)
                {
                    continue;
                }
                count += 1;
                total += u64::from(s.response);
            }
        }
        ValidationSummaryView {
            count: U64(count),
            average_response: if count > 0 { (total / count) as u8 } else { 0 },
        }
    }

    pub fn get_agent_validations(
        &self,
        agent_id: String,
        from_index: Option<u64>,
        limit: Option<u64>,
    ) -> Vec<String> {
        let Some(id) = parse_u64(&agent_id) else {
            return vec![];
        };
        self.agent_validations
            .get(&id)
            .map(|l| page(l, from_index, limit))
            .unwrap_or_default()
    }

    pub fn get_validator_requests(
        &self,
        validator_address: AccountId,
        from_index: Option<u64>,
        limit: Option<u64>,
    ) -> Vec<String> {
        self.validator_requests
            .get(&validator_address)
            .map(|l| page(l, from_index, limit))
            .unwrap_or_default()
    }

    pub fn is_request_pending(&self, request_hash: String) -> bool {
        parse_hash(&request_hash)
            .and_then(|h| self.pending.get(&h).copied())
            .is_some_and(|at| env::block_height() < at.saturating_add(PENDING_TTL_BLOCKS))
    }

    pub fn get_identity_registry(&self) -> AccountId {
        self.identity_registry.clone()
    }

    pub fn get_version(&self) -> String {
        erc8004_common::ERC8004_VERSION.into()
    }

    pub fn get_config(&self) -> near_sdk::serde_json::Value {
        near_sdk::serde_json::json!({
            "contract": "erc8004-validation",
            "status": "BORRADOR",
            "erc8004_version": erc8004_common::ERC8004_VERSION,
            "identity_registry": self.identity_registry,
            "request_deposit": U128(bytes_cost(REQUEST_MAX_BYTES)),
            "min_gas_validation_request": U64(MIN_GAS_VALIDATION_REQUEST.as_gas()),
        })
    }
}
