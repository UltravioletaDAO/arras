//! Los esquemas JSON de las llamadas cruzadas. Los escribe un contrato y los lee otro: al leer, son **estrictos**
//! (`deny_unknown_fields`, ningun campo de menos, sin claves repetidas, los enteros como decimales canonicos), y lo
//! que no cumple se rechaza en el callback (falla cerrado).

use near_sdk::json_types::U128;
use near_sdk::serde::de::Error as _;
use near_sdk::serde::{Deserialize, Deserializer, Serialize};
use near_sdk::{AccountId, serde_json};

use crate::validate::{is_canonical_uint, is_valid_label};

/// Tope de bytes de la respuesta de `check_agent` que se lee en un callback (una respuesta real ocupa < 200).
pub const MAX_AGENT_CHECK_LEN: usize = 1024;
/// Tope de bytes de la respuesta de `verify_feedback_proof` que se lee en un callback (una real ocupa < 300).
pub const MAX_FEEDBACK_PROOF_LEN: usize = 1024;

/// Lo que devuelve `identity.check_agent(agent_id, account_id)` cuando el agente existe (si no, `null`).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(crate = "near_sdk::serde", deny_unknown_fields)]
pub struct AgentCheck {
    /// El dueno del NFT.
    pub owner_id: AccountId,
    /// La `agentWallet` (o `null`). El campo tiene que estar (con `null` si no hay).
    #[serde(deserialize_with = "required_nullable")]
    pub agent_wallet: Option<AccountId>,
    /// `account_id` es el dueno o una cuenta aprobada (NEP-178) para este agente.
    pub authorized: bool,
}

/// Lo que devuelve `verifier.verify_feedback_proof(client, proof)`: la otra parte del trato que prueba `proof`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(crate = "near_sdk::serde", deny_unknown_fields)]
pub struct FeedbackProof {
    /// La otra parte del trato (el Reputation Registry exige que sea el dueno o la `agentWallet` del agente).
    pub counterparty: AccountId,
    /// El rol de `counterparty` en el trato (Arras: `payer` o `receiver`). `[a-z0-9_-]{0,32}`.
    #[serde(deserialize_with = "label")]
    pub role: String,
    /// Etiqueta del verificador (Arras: el `operator_id` del pago). `[a-z0-9_-]{0,32}`.
    #[serde(deserialize_with = "label")]
    pub context: String,
    /// Peso del trato (Arras: `released`, en unidades del token). Decimal canonico.
    #[serde(deserialize_with = "canonical_u128")]
    pub weight: U128,
}

/// Un `Option` que tiene que estar presente (`null` vale; ausente, no).
fn required_nullable<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    Option::<T>::deserialize(d)
}

fn label<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let s = String::deserialize(d)?;
    if !is_valid_label(&s) {
        return Err(D::Error::custom("label invalido"));
    }
    Ok(s)
}

fn canonical_u128<'de, D: Deserializer<'de>>(d: D) -> Result<U128, D::Error> {
    let s = String::deserialize(d)?;
    if !is_canonical_uint(&s) {
        return Err(D::Error::custom("numero no canonico"));
    }
    s.parse::<u128>()
        .map(U128)
        .map_err(|_| D::Error::custom("numero fuera de rango"))
}

/// Una respuesta que no tiene el esquema esperado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidResponse;

/// Lee la respuesta de `check_agent`: `Ok(None)` es `null` (el agente no existe); `Err` es que no es exactamente un
/// `AgentCheck` ni `null`.
pub fn parse_agent_check(bytes: &[u8]) -> Result<Option<AgentCheck>, InvalidResponse> {
    serde_json::from_slice::<Option<AgentCheck>>(bytes).map_err(|_| InvalidResponse)
}

/// Lee la respuesta de `verify_feedback_proof`: exactamente un `FeedbackProof` (`null` no vale).
pub fn parse_feedback_proof(bytes: &[u8]) -> Result<FeedbackProof, InvalidResponse> {
    serde_json::from_slice::<FeedbackProof>(bytes).map_err(|_| InvalidResponse)
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::serde_json::json;

    fn bytes(v: near_sdk::serde_json::Value) -> Vec<u8> {
        serde_json::to_vec(&v).unwrap()
    }

    #[test]
    fn agent_check_estricto() {
        let ok = json!({"owner_id":"a.near","agent_wallet":"w.near","authorized":false});
        assert_eq!(
            parse_agent_check(&bytes(ok)).unwrap().unwrap().agent_wallet,
            Some("w.near".parse().unwrap())
        );
        let ok_null = json!({"owner_id":"a.near","agent_wallet":null,"authorized":true});
        assert!(
            parse_agent_check(&bytes(ok_null))
                .unwrap()
                .unwrap()
                .authorized
        );
        assert_eq!(parse_agent_check(b"null"), Ok(None));
        for bad in [
            json!({"owner_id":"a.near","authorized":false}),
            json!({"agent_wallet":null,"authorized":false}),
            json!({"owner_id":"a.near","agent_wallet":null}),
            json!({"owner_id":"a.near","agent_wallet":null,"authorized":false,"x":1}),
            json!({"owner_id":"A B","agent_wallet":null,"authorized":false}),
            json!({"owner_id":"a.near","agent_wallet":null,"authorized":"false"}),
            json!({"owner_id":"a.near","agent_wallet":null,"authorized":0}),
            json!({"owner_id":"a.near","agent_wallet":1,"authorized":false}),
            json!(true),
            json!([]),
        ] {
            assert!(parse_agent_check(&bytes(bad.clone())).is_err(), "{bad}");
        }
        assert!(parse_agent_check(b"").is_err());
        assert!(parse_agent_check(b"{").is_err());
        let dup =
            br#"{"owner_id":"a.near","owner_id":"b.near","agent_wallet":null,"authorized":false}"#;
        assert!(parse_agent_check(dup).is_err());
    }

    #[test]
    fn feedback_proof_estricto() {
        let ok =
            json!({"counterparty":"w.near","role":"receiver","context":"em","weight":"100000"});
        let p = parse_feedback_proof(&bytes(ok)).unwrap();
        assert_eq!(p.weight, U128(100_000));
        let vacios = json!({"counterparty":"w.near","role":"","context":"","weight":"0"});
        assert!(parse_feedback_proof(&bytes(vacios)).is_ok());
        for bad in [
            json!(null),
            json!({"counterparty":"w.near","role":"receiver","context":"em"}),
            json!({"counterparty":"w.near","role":"receiver","weight":"1"}),
            json!({"counterparty":"w.near","context":"em","weight":"1"}),
            json!({"role":"receiver","context":"em","weight":"1"}),
            json!({"counterparty":"w.near","role":"receiver","context":"em","weight":"1","x":0}),
            json!({"counterparty":"w.near","role":"Receiver","context":"em","weight":"1"}),
            json!({"counterparty":"w.near","role":"receiver","context":"e m","weight":"1"}),
            json!({"counterparty":"w.near","role":"receiver","context":"em","weight":"01"}),
            json!({"counterparty":"w.near","role":"receiver","context":"em","weight":"+1"}),
            json!({"counterparty":"w.near","role":"receiver","context":"em","weight":1}),
            json!({"counterparty":"w.near","role":"receiver","context":"em","weight":"340282366920938463463374607431768211456"}),
            json!({"counterparty":"W","role":"receiver","context":"em","weight":"1"}),
            json!({"counterparty":"w.near","role":"r".repeat(33),"context":"em","weight":"1"}),
        ] {
            assert!(parse_feedback_proof(&bytes(bad.clone())).is_err(), "{bad}");
        }
    }
}
