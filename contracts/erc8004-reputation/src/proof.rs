//! Extension `feedback-proofs` (seccion 3.3 del plan, opcion A elegida por el dueno): una calificacion con la prueba en
//! cadena de un trato entre el cliente y el agente, dada por un **verificador** (cualquier contrato: no hay lista; quien
//! lee filtra por el verificador en que confia).
//!
//! `give_feedback_with_proof`: las guardas de `give_feedback` y ademas `E40` proof -> `E41` verifier (no puede ser el
//! propio registro) -> `E09` deposito (`feedback_proof_deposit`) -> `E10` gas -> `E42` ya califico con esa prueba ->
//! `E43` hay otra en vuelo. Pone el candado y llama **en paralelo** a `identity.check_agent` y a
//! `verifier.verify_feedback_proof(client, proof)`.
//!
//! `on_feedback_proof_checked` (nunca entra en panico): `E44` el candado ya no es de esta llamada -> `E20`..`E23`
//! (la identidad, como en el nucleo) -> `E45` el verificador fallo -> `E46` su respuesta no es exactamente un
//! `FeedbackProof` -> `E47` la contraparte no es el dueno ni la `agentWallet` del agente -> `E48` el cliente es la
//! `agentWallet` del agente -> `E42` ya califico con esa prueba.

use crate::events::ProofData;
use crate::feedback::ext_identity;
use crate::*;
use erc8004_common::types::{
    FeedbackProof, MAX_AGENT_CHECK_LEN, MAX_FEEDBACK_PROOF_LEN, parse_feedback_proof,
};
use erc8004_common::validate::is_valid_proof;
use near_sdk::json_types::{U64, U128};
use near_sdk::{Promise, PromiseError, ext_contract};

/// La interfaz de un verificador. El trait solo genera `ext_verifier`.
#[allow(dead_code)]
#[ext_contract(ext_verifier)]
pub trait FeedbackProofVerifier {
    fn verify_feedback_proof(&self, client: AccountId, proof: String) -> FeedbackProof;
}

/// Lo que se guarda de una prueba aceptada.
#[near(serializers = [borsh])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredProof {
    pub verifier: AccountId,
    pub proof: String,
    pub counterparty: AccountId,
    pub role: String,
    pub context: String,
    pub weight: u128,
}

/// La prueba verificada y su clave, de camino a `accept`.
pub(crate) struct ProofToStore {
    pub key: [u8; 32],
    pub proof: StoredProof,
}

/// La clave de una prueba: sha256 de borsh((verificador, proof, cliente)) (borsh lleva el largo de cada string
/// adelante: la tupla no es ambigua).
pub fn proof_key(verifier: &AccountId, proof: &str, client: &AccountId) -> [u8; 32] {
    let bytes =
        near_sdk::borsh::to_vec(&(verifier, proof, client)).unwrap_or_else(|_| env::abort());
    env::sha256_array(&bytes)
}

/// Prefijo del `Vector` de calificaciones verificadas de un `(agente, verificador)`.
pub(crate) fn verified_prefix(agent_id: u64, verifier: &AccountId) -> Vec<u8> {
    let bytes = near_sdk::borsh::to_vec(&(agent_id, verifier)).unwrap_or_else(|_| env::abort());
    let mut p = vec![prefix::VERIFIED_LIST];
    p.extend_from_slice(&env::sha256_array(&bytes));
    p
}

impl ProofData {
    pub(crate) fn of(p: &StoredProof) -> Self {
        Self {
            verifier: p.verifier.clone(),
            proof: p.proof.clone(),
            counterparty: p.counterparty.clone(),
            role: p.role.clone(),
            context: p.context.clone(),
            weight: U128(p.weight),
        }
    }
}

impl ReputationRegistry {
    /// Guarda la prueba de la calificacion `(id, client, index)`, la marca como usada y la indexa por verificador.
    pub(crate) fn store_proof(
        &mut self,
        id: u64,
        client: &AccountId,
        index: u64,
        p: ProofToStore,
    ) -> ProofData {
        let data = ProofData::of(&p.proof);
        let verifier = p.proof.verifier.clone();
        self.proofs.insert((id, client.clone(), index), p.proof);
        self.proof_used.insert(p.key, (id, index));
        let list = self
            .verified
            .entry((id, verifier.clone()))
            .or_insert_with(|| Vector::new(verified_prefix(id, &verifier)));
        list.push((client.clone(), index));
        list.flush();
        data
    }

    /// Las guardas del callback sobre las dos respuestas, sin escribir nada.
    pub(crate) fn check_proof(
        &self,
        p: &PendingFeedback,
        verifier: &AccountId,
        proof: &str,
        key: &[u8; 32],
        identity: Result<Vec<u8>, PromiseError>,
        verified: Result<Vec<u8>, PromiseError>,
    ) -> Result<StoredProof, ReputationError> {
        let agent = Self::check_identity(identity)?;
        let bytes = match verified {
            Ok(b) => b,
            Err(PromiseError::Failed) => return Err(ReputationError::E45VerificadorFallo),
            Err(_) => return Err(ReputationError::E46RespuestaDelVerificadorInvalida),
        };
        let fp = parse_feedback_proof(&bytes)
            .map_err(|_| ReputationError::E46RespuestaDelVerificadorInvalida)?;
        let is_agent = fp.counterparty == agent.owner_id
            || agent.agent_wallet.as_ref() == Some(&fp.counterparty);
        if !is_agent {
            return Err(ReputationError::E47NoEsElAgente);
        }
        if agent.agent_wallet.as_ref() == Some(&p.client) {
            return Err(ReputationError::E48AgentWalletCalifica);
        }
        if self.proof_used.contains_key(key) {
            return Err(ReputationError::E42PruebaYaUsada);
        }
        Ok(StoredProof {
            verifier: verifier.clone(),
            proof: proof.to_string(),
            counterparty: fp.counterparty,
            role: fp.role,
            context: fp.context,
            weight: fp.weight.0,
        })
    }
}

#[near]
impl ReputationRegistry {
    /// `give_feedback` con una prueba: `verifier.verify_feedback_proof(client, proof)` tiene que devolver una
    /// contraparte que sea el dueno o la `agentWallet` del agente. Una calificacion por `(verifier, proof, cliente)`.
    /// Adjuntar `feedback_proof_deposit` y al menos `min_gas_give_feedback_with_proof` (`get_config`).
    #[payable]
    #[allow(clippy::too_many_arguments)]
    pub fn give_feedback_with_proof(
        &mut self,
        agent_id: String,
        value: String,
        value_decimals: u8,
        tag1: Option<String>,
        tag2: Option<String>,
        endpoint: Option<String>,
        feedback_uri: Option<String>,
        feedback_hash: Option<String>,
        verifier: AccountId,
        proof: String,
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
        if !is_valid_proof(&proof) {
            ReputationError::E40ProofInvalida.panic();
        }
        if verifier == env::current_account_id() {
            ReputationError::E41VerifierInvalido.panic();
        }
        Self::require_deposit_and_gas(
            Self::feedback_proof_deposit(),
            MIN_GAS_GIVE_FEEDBACK_WITH_PROOF,
        );
        let key = proof_key(&verifier, &proof, &p.client);
        if self.proof_used.contains_key(&key) {
            ReputationError::E42PruebaYaUsada.panic();
        }
        let height = env::block_height();
        if let Some(&at) = self.pending.get(&key)
            && height < at.saturating_add(PENDING_TTL_BLOCKS)
        {
            ReputationError::E43PruebaEnVuelo.panic();
        }
        self.pending.insert(key, height);
        ext_identity::ext(self.identity_registry.clone())
            .with_static_gas(GAS_CHECK_AGENT)
            .check_agent(p.agent_id.clone(), p.client.clone())
            .and(
                ext_verifier::ext(verifier.clone())
                    .with_static_gas(GAS_VERIFY_PROOF)
                    .verify_feedback_proof(p.client.clone(), proof.clone()),
            )
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(GAS_ON_PROOF_CHECKED)
                    .on_feedback_proof_checked(p, verifier, proof, U64(height)),
            )
    }

    /// Callback de `give_feedback_with_proof`. Lee primero los dos resultados.
    #[private]
    pub fn on_feedback_proof_checked(
        &mut self,
        p: PendingFeedback,
        verifier: AccountId,
        proof: String,
        nonce: U64,
    ) -> Option<String> {
        let identity = env::promise_result_checked(0, MAX_AGENT_CHECK_LEN);
        let verified = env::promise_result_checked(1, MAX_FEEDBACK_PROOF_LEN);
        let key = proof_key(&verifier, &proof, &p.client);
        let outcome = if self.pending.get(&key) == Some(&nonce.0) {
            self.pending.remove(&key);
            self.check_proof(&p, &verifier, &proof, &key, identity, verified)
        } else {
            Err(ReputationError::E44Reemplazado)
        };
        match outcome {
            Ok(stored) => Some(self.accept(p, Some(ProofToStore { key, proof: stored }))),
            Err(e) => Self::reject(&p, e),
        }
    }
}

/// Bytes del candado mientras vuela (lo borra el callback).
pub const PENDING_RECORD_BYTES: u64 = erc8004_common::storage::record_bytes(1 + 32, 8);
