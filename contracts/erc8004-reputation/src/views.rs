//! Vistas: las de EVM (`getSummary`, `readFeedback`, `readAllFeedback`, `getResponseCount`, `getClients`,
//! `getLastIndex`, `getIdentityRegistry`, `getVersion`) y, con `feedback-proofs`, las de las pruebas.

use crate::*;
use erc8004_common::summary::Accumulator;
use erc8004_common::validate::parse_u64;
use near_sdk::json_types::{U64, U128};
use near_sdk::serde::Serialize;

/// `getSummary`: `(count, summaryValue, summaryValueDecimals)`. `summary_value` es exacto (desviacion R-4).
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(crate = "near_sdk::serde")]
pub struct SummaryView {
    pub count: U64,
    pub summary_value: String,
    pub summary_value_decimals: u8,
}

/// `readFeedback`.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(crate = "near_sdk::serde")]
pub struct FeedbackView {
    pub value: String,
    pub value_decimals: u8,
    pub tag1: String,
    pub tag2: String,
    pub is_revoked: bool,
}

/// `readAllFeedback`: los 7 arreglos paralelos de EVM.
#[derive(Serialize, Debug, Clone, Default, PartialEq, Eq)]
#[serde(crate = "near_sdk::serde")]
pub struct AllFeedbackView {
    pub clients: Vec<AccountId>,
    pub feedback_indexes: Vec<String>,
    pub values: Vec<String>,
    pub value_decimals: Vec<u8>,
    pub tag1s: Vec<String>,
    pub tag2s: Vec<String>,
    pub revoked_statuses: Vec<bool>,
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(crate = "near_sdk::serde")]
pub struct ReputationConfig {
    pub contract: String,
    pub erc8004_version: String,
    pub events_standard: String,
    pub events_version: String,
    pub identity_registry: AccountId,
    pub feedback_deposit: U128,
    pub min_gas_give_feedback: U64,
    /// `null` sin el feature `feedback-proofs`.
    pub feedback_proof_deposit: Option<U128>,
    pub min_gas_give_feedback_with_proof: Option<U64>,
    pub max_endpoint_len: u64,
    pub max_feedback_uri_len: u64,
    pub max_response_uri_len: u64,
    pub max_tag_len: u64,
    pub max_list_limit: u64,
}

/// `""` o ausente = sin filtro (como el `keccak256("")` de EVM).
fn tag_matches(filter: &Option<String>, tag: &str) -> bool {
    match filter.as_deref() {
        None | Some("") => true,
        Some(f) => f == tag,
    }
}

impl ReputationRegistry {
    pub(crate) fn feedback_at(&self, id: u64, client: &AccountId, index: u64) -> Option<&Feedback> {
        self.feedback.get(&(id, client.clone(), index))
    }

    fn client_list(&self, id: u64) -> Vec<AccountId> {
        self.clients
            .get(&id)
            .map(|v| v.iter().cloned().collect())
            .unwrap_or_default()
    }

    fn count_responses(
        &self,
        id: u64,
        client: &AccountId,
        index: u64,
        responders: &Option<Vec<AccountId>>,
    ) -> u64 {
        let get = |r: &AccountId| {
            self.response_count
                .get(&(id, client.clone(), index, r.clone()))
                .copied()
                .unwrap_or(0)
        };
        match responders.as_deref() {
            Some(list) if !list.is_empty() => list.iter().map(get).fold(0u64, u64::saturating_add),
            _ => self
                .responders
                .get(&(id, client.clone(), index))
                .map(|v| v.iter().map(get).fold(0u64, u64::saturating_add))
                .unwrap_or(0),
        }
    }
}

#[near]
impl ReputationRegistry {
    pub fn get_identity_registry(&self) -> AccountId {
        self.identity_registry.clone()
    }

    /// `getSummary`. `client_addresses` no puede estar vacia (`E30`, "clientAddresses required": sin filtro de
    /// clientes el resumen es atacable por sybil). Un cliente repetido cuenta dos veces, como en EVM. Sin revocadas.
    pub fn get_summary(
        &self,
        agent_id: String,
        client_addresses: Vec<AccountId>,
        tag1: Option<String>,
        tag2: Option<String>,
    ) -> SummaryView {
        let id = parse_agent_id(&agent_id);
        if client_addresses.is_empty() {
            ReputationError::E30ClientesRequeridos.panic();
        }
        let mut acc = Accumulator::default();
        for client in &client_addresses {
            for j in 1..=self.last(id, client) {
                let Some(fb) = self.feedback_at(id, client, j) else {
                    continue;
                };
                if fb.is_revoked || !tag_matches(&tag1, &fb.tag1) || !tag_matches(&tag2, &fb.tag2) {
                    continue;
                }
                acc.add(fb.value, fb.value_decimals);
            }
        }
        let s = acc.finish();
        SummaryView {
            count: U64(s.count),
            summary_value: s.value,
            summary_value_decimals: s.decimals,
        }
    }

    /// `readFeedback`: `null` si el indice es 0, no es canonico o no existe.
    pub fn read_feedback(
        &self,
        agent_id: String,
        client_address: AccountId,
        feedback_index: String,
    ) -> Option<FeedbackView> {
        let id = parse_u64(&agent_id)?;
        let index = parse_u64(&feedback_index)?;
        self.feedback_at(id, &client_address, index)
            .map(|fb| FeedbackView {
                value: fb.value.to_string(),
                value_decimals: fb.value_decimals,
                tag1: fb.tag1.clone(),
                tag2: fb.tag2.clone(),
                is_revoked: fb.is_revoked,
            })
    }

    /// `readAllFeedback`: sin `client_addresses` (o vacia), todos los clientes del agente; sin revocadas salvo
    /// `include_revoked`.
    pub fn read_all_feedback(
        &self,
        agent_id: String,
        client_addresses: Option<Vec<AccountId>>,
        tag1: Option<String>,
        tag2: Option<String>,
        include_revoked: Option<bool>,
    ) -> AllFeedbackView {
        let id = parse_agent_id(&agent_id);
        let include_revoked = include_revoked.unwrap_or(false);
        let clients = match client_addresses {
            Some(c) if !c.is_empty() => c,
            _ => self.client_list(id),
        };
        let mut out = AllFeedbackView::default();
        for client in &clients {
            for j in 1..=self.last(id, client) {
                let Some(fb) = self.feedback_at(id, client, j) else {
                    continue;
                };
                if (!include_revoked && fb.is_revoked)
                    || !tag_matches(&tag1, &fb.tag1)
                    || !tag_matches(&tag2, &fb.tag2)
                {
                    continue;
                }
                out.clients.push(client.clone());
                out.feedback_indexes.push(j.to_string());
                out.values.push(fb.value.to_string());
                out.value_decimals.push(fb.value_decimals);
                out.tag1s.push(fb.tag1.clone());
                out.tag2s.push(fb.tag2.clone());
                out.revoked_statuses.push(fb.is_revoked);
            }
        }
        out
    }

    /// `getResponseCount`: sin `client_address`, todos los clientes; con `feedback_index` ausente o `"0"`, todas las
    /// del cliente; sin `responders` (o vacia), todos los respondedores.
    pub fn get_response_count(
        &self,
        agent_id: String,
        client_address: Option<AccountId>,
        feedback_index: Option<String>,
        responders: Option<Vec<AccountId>>,
    ) -> U64 {
        let id = parse_agent_id(&agent_id);
        let index = feedback_index.as_deref().map(parse_index).unwrap_or(0);
        let mut count = 0u64;
        match client_address {
            None => {
                for client in self.client_list(id) {
                    for j in 1..=self.last(id, &client) {
                        count =
                            count.saturating_add(self.count_responses(id, &client, j, &responders));
                    }
                }
            }
            Some(client) if index == 0 => {
                for j in 1..=self.last(id, &client) {
                    count = count.saturating_add(self.count_responses(id, &client, j, &responders));
                }
            }
            Some(client) => count = self.count_responses(id, &client, index, &responders),
        }
        U64(count)
    }

    /// `getClients`: sin paginar, todos (como EVM); con `from_index`/`limit`, una pagina de hasta 100.
    pub fn get_clients(
        &self,
        agent_id: String,
        from_index: Option<u64>,
        limit: Option<u64>,
    ) -> Vec<AccountId> {
        let id = parse_agent_id(&agent_id);
        let Some(list) = self.clients.get(&id) else {
            return vec![];
        };
        if from_index.is_none() && limit.is_none() {
            return list.iter().cloned().collect();
        }
        let from = from_index.unwrap_or(0);
        let limit = limit.unwrap_or(MAX_LIST_LIMIT).min(MAX_LIST_LIMIT);
        let end = u64::from(list.len()).min(from.saturating_add(limit));
        (from..end)
            .filter_map(|i| list.get(i as u32).cloned())
            .collect()
    }

    /// `getLastIndex`.
    pub fn get_last_index(&self, agent_id: String, client_address: AccountId) -> U64 {
        let id = parse_agent_id(&agent_id);
        U64(self.last(id, &client_address))
    }

    pub fn get_version(&self) -> String {
        erc8004_common::ERC8004_VERSION.into()
    }

    pub fn get_config(&self) -> ReputationConfig {
        let proofs = cfg!(feature = "feedback-proofs");
        ReputationConfig {
            contract: "erc8004-reputation".into(),
            erc8004_version: erc8004_common::ERC8004_VERSION.into(),
            events_standard: erc8004_common::events::STANDARD.into(),
            events_version: erc8004_common::events::VERSION.into(),
            identity_registry: self.identity_registry.clone(),
            feedback_deposit: U128(Self::feedback_deposit()),
            min_gas_give_feedback: U64(MIN_GAS_GIVE_FEEDBACK.as_gas()),
            feedback_proof_deposit: proofs.then(|| U128(Self::feedback_proof_deposit())),
            min_gas_give_feedback_with_proof: proofs
                .then(|| U64(MIN_GAS_GIVE_FEEDBACK_WITH_PROOF.as_gas())),
            max_endpoint_len: MAX_ENDPOINT_LEN as u64,
            max_feedback_uri_len: MAX_FEEDBACK_URI_LEN as u64,
            max_response_uri_len: MAX_RESPONSE_URI_LEN as u64,
            max_tag_len: erc8004_common::validate::MAX_TAG_LEN as u64,
            max_list_limit: MAX_LIST_LIMIT,
        }
    }
}

/// Las vistas de las pruebas.
#[cfg(feature = "feedback-proofs")]
mod proof_views {
    use super::*;
    use crate::events::ProofData;
    use crate::proof::proof_key;

    #[derive(Serialize, Debug, Clone, PartialEq, Eq)]
    #[serde(crate = "near_sdk::serde")]
    pub struct FeedbackRef {
        pub agent_id: String,
        pub client_address: AccountId,
        pub feedback_index: String,
    }

    /// `get_verified_summary`: el resumen de las calificaciones con prueba de un verificador, sin revocadas.
    #[derive(Serialize, Debug, Clone, PartialEq, Eq)]
    #[serde(crate = "near_sdk::serde")]
    pub struct VerifiedSummaryView {
        pub count: U64,
        pub summary_value: String,
        pub summary_value_decimals: u8,
        /// La suma de los `weight` (Arras: lo liberado), exacta.
        pub weight_sum: String,
    }

    #[near]
    impl ReputationRegistry {
        /// La prueba de una calificacion, o `null` si no tiene.
        pub fn read_feedback_proof(
            &self,
            agent_id: String,
            client_address: AccountId,
            feedback_index: String,
        ) -> Option<ProofData> {
            let id = parse_u64(&agent_id)?;
            let index = parse_u64(&feedback_index)?;
            self.proofs
                .get(&(id, client_address, index))
                .map(ProofData::of)
        }

        /// La calificacion que uso `(verifier, proof, client_address)`, o `null`.
        pub fn get_proof_feedback(
            &self,
            verifier: AccountId,
            proof: String,
            client_address: AccountId,
        ) -> Option<FeedbackRef> {
            let &(id, index) =
                self.proof_used
                    .get(&proof_key(&verifier, &proof, &client_address))?;
            Some(FeedbackRef {
                agent_id: id.to_string(),
                client_address,
                feedback_index: index.to_string(),
            })
        }

        /// Si hay un `give_feedback_with_proof` en vuelo con esa prueba (y su candado no vencio).
        pub fn is_proof_pending(
            &self,
            verifier: AccountId,
            proof: String,
            client_address: AccountId,
        ) -> bool {
            self.pending
                .get(&proof_key(&verifier, &proof, &client_address))
                .is_some_and(|&at| env::block_height() < at.saturating_add(PENDING_TTL_BLOCKS))
        }

        /// Las calificaciones con prueba de `verifier` para el agente, en orden de llegada (revocadas incluidas).
        pub fn list_verified_feedback(
            &self,
            agent_id: String,
            verifier: AccountId,
            from_index: Option<u64>,
            limit: Option<u64>,
        ) -> Vec<FeedbackRef> {
            let id = parse_agent_id(&agent_id);
            let Some(list) = self.verified.get(&(id, verifier)) else {
                return vec![];
            };
            let from = from_index.unwrap_or(0);
            let limit = limit.unwrap_or(MAX_LIST_LIMIT).min(MAX_LIST_LIMIT);
            let end = u64::from(list.len()).min(from.saturating_add(limit));
            (from..end)
                .filter_map(|i| list.get(i as u32))
                .map(|(client, index)| FeedbackRef {
                    agent_id: id.to_string(),
                    client_address: client.clone(),
                    feedback_index: index.to_string(),
                })
                .collect()
        }

        /// El resumen (el mismo algoritmo que `get_summary`) de las calificaciones con prueba de `verifier`, vigentes.
        /// `client_addresses` es opcional (una calificacion con prueba ya costo un trato real; quien lee igual puede
        /// filtrar); se usa como conjunto. `context`: ausente = sin filtro; si viene, igual exacto (tambien `""`).
        pub fn get_verified_summary(
            &self,
            agent_id: String,
            verifier: AccountId,
            client_addresses: Option<Vec<AccountId>>,
            tag1: Option<String>,
            tag2: Option<String>,
            context: Option<String>,
        ) -> VerifiedSummaryView {
            let id = parse_agent_id(&agent_id);
            let mut acc = Accumulator::default();
            let mut weight = erc8004_common::summary::U256::ZERO;
            let clients = client_addresses.filter(|c| !c.is_empty());
            if let Some(list) = self.verified.get(&(id, verifier)) {
                for (client, index) in list.iter() {
                    if clients.as_ref().is_some_and(|c| !c.contains(client)) {
                        continue;
                    }
                    let Some(fb) = self.feedback_at(id, client, *index) else {
                        continue;
                    };
                    if fb.is_revoked
                        || !tag_matches(&tag1, &fb.tag1)
                        || !tag_matches(&tag2, &fb.tag2)
                    {
                        continue;
                    }
                    let Some(pr) = self.proofs.get(&(id, client.clone(), *index)) else {
                        continue;
                    };
                    if context.as_ref().is_some_and(|c| c != &pr.context) {
                        continue;
                    }
                    acc.add(fb.value, fb.value_decimals);
                    weight = weight
                        .checked_add(erc8004_common::summary::U256::from_u128(pr.weight))
                        .unwrap_or_else(|| env::abort());
                }
            }
            let s = acc.finish();
            VerifiedSummaryView {
                count: U64(s.count),
                summary_value: s.value,
                summary_value_decimals: s.decimals,
                weight_sum: weight.to_dec_string(),
            }
        }
    }
}

#[cfg(feature = "feedback-proofs")]
pub use proof_views::*;
