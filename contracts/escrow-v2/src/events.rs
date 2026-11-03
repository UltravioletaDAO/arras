//! Eventos NEP-297 que v2 suma (prescripcion sec. 4), con el mismo `standard` (`arras`) y la misma `version`
//! (`1.0.0`) que v1: los eventos de v1 (`authorized`, `released`, ...) no cambian de forma y v2 los emite con
//! [`crate::events::ArrasEvent`]. `operator_registered` y `operator_updated` de v2 son los de v1 con campos de mas
//! (`arbiter`, `arbiter_window_ns`, `ticket_pubkey`): un lector de v1 los sigue leyendo.

use near_sdk::json_types::{U64, U128};
use near_sdk::serde::Serialize;
use near_sdk::{AccountId, PublicKey, near};

/// `operator_registered` (todos los campos) y `operator_updated` (solo los que cambiaron) de v2.
#[derive(Serialize, Debug, Clone, PartialEq, Eq, Default)]
#[serde(crate = "near_sdk::serde")]
pub struct OperatorDataV2 {
    pub operator_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<AccountId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capturers: Option<Vec<AccountId>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee_bps: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee_recipient: Option<AccountId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_amount: Option<U128>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tope_tvl_operador: Option<U128>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tope_tvl_por_payer: Option<U128>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sponsor_max_por_payer: Option<U128>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arbiter: Option<AccountId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arbiter_window_ns: Option<U64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ticket_pubkey: Option<PublicKey>,
    /// `true` cuando `update_operator` quita la llave del ticket.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quitar_ticket_pubkey: Option<bool>,
}

/// `budget_deposited`: lo depositado y como queda el presupuesto (topes ya con el `min` de viejo y nuevo).
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(crate = "near_sdk::serde")]
pub struct BudgetDepositedData {
    pub owner: AccountId,
    pub agent: AccountId,
    pub operator_id: String,
    pub amount: U128,
    pub saldo: U128,
    pub tope_por_pago: U128,
    pub tope_diario: U128,
}

/// `budget_spent`: el agente autorizo un pago desde el presupuesto (va junto al `authorized` del pago).
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(crate = "near_sdk::serde")]
pub struct BudgetSpentData {
    pub owner: AccountId,
    pub agent: AccountId,
    pub operator_id: String,
    pub payment_id: String,
    pub amount: U128,
    pub saldo: U128,
    pub gastado_hoy: U128,
}

/// `budget_withdrawn` y `agent_revoked`: lo que vuelve al `claimable` del owner.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(crate = "near_sdk::serde")]
pub struct BudgetAmountData {
    pub owner: AccountId,
    pub agent: AccountId,
    pub operator_id: String,
    pub amount: U128,
}

/// `budget_caps_set`: los topes nuevos (los pone el owner con 1 yocto).
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(crate = "near_sdk::serde")]
pub struct BudgetCapsData {
    pub owner: AccountId,
    pub agent: AccountId,
    pub operator_id: String,
    pub tope_por_pago: U128,
    pub tope_diario: U128,
}

/// `dispute_opened`.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(crate = "near_sdk::serde")]
pub struct DisputeOpenedData {
    pub payment_id: String,
    pub opened_by: AccountId,
    pub reason_hash: String,
    /// Lo vivo que queda congelado.
    pub amount: U128,
    /// `opened_at + arbiter_window_ns`: desde ahi cualquiera llama `resolve_expired_dispute`.
    pub resolve_by: U64,
}

/// `dispute_resolved`: `to_receiver` se reparte como un `release` (`net` + `fee`), `to_payer` va al payer.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(crate = "near_sdk::serde")]
pub struct DisputeResolvedData {
    pub payment_id: String,
    pub by: AccountId,
    pub to_receiver: U128,
    pub net: U128,
    pub fee: U128,
    pub to_payer: U128,
    /// `true` si lo resolvio el vencimiento del plazo del arbitro.
    pub expired: bool,
}

#[near(event_json(standard = "arras"))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArrasEventV2 {
    #[event_version("1.0.0")]
    OperatorRegistered(Vec<OperatorDataV2>),
    #[event_version("1.0.0")]
    OperatorUpdated(Vec<OperatorDataV2>),
    #[event_version("1.0.0")]
    BudgetDeposited(Vec<BudgetDepositedData>),
    #[event_version("1.0.0")]
    BudgetSpent(Vec<BudgetSpentData>),
    #[event_version("1.0.0")]
    BudgetWithdrawn(Vec<BudgetAmountData>),
    #[event_version("1.0.0")]
    BudgetCapsSet(Vec<BudgetCapsData>),
    #[event_version("1.0.0")]
    AgentRevoked(Vec<BudgetAmountData>),
    #[event_version("1.0.0")]
    DisputeOpened(Vec<DisputeOpenedData>),
    #[event_version("1.0.0")]
    DisputeResolved(Vec<DisputeResolvedData>),
}

/// El log completo, `EVENT_JSON:{...}`, sin emitirlo (para tests y para el SDK).
pub fn event_log_v2(e: &ArrasEventV2) -> String {
    format!(
        "EVENT_JSON:{}",
        near_sdk::serde_json::to_string(&e.to_json()).unwrap_or_default()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn acc(s: &str) -> AccountId {
        s.parse().unwrap()
    }

    #[test]
    fn formato_exacto_y_nombres() {
        let r = ArrasEventV2::DisputeResolved(vec![DisputeResolvedData {
            payment_id: "DN5eMA3sNES7xsoy6qvy24cVF6Dmwjxpkvd2b4e8P38M".into(),
            by: acc("arbiter.testnet"),
            to_receiver: U128(400_000),
            net: U128(348_000),
            fee: U128(52_000),
            to_payer: U128(600_000),
            expired: false,
        }]);
        assert_eq!(
            event_log_v2(&r),
            r#"EVENT_JSON:{"standard":"arras","version":"1.0.0","event":"dispute_resolved","data":[{"payment_id":"DN5eMA3sNES7xsoy6qvy24cVF6Dmwjxpkvd2b4e8P38M","by":"arbiter.testnet","to_receiver":"400000","net":"348000","fee":"52000","to_payer":"600000","expired":false}]}"#
        );
        let b = BudgetAmountData {
            owner: acc("human.testnet"),
            agent: acc("agent.testnet"),
            operator_id: "em".into(),
            amount: U128(1),
        };
        let casos = [
            (
                ArrasEventV2::BudgetWithdrawn(vec![b.clone()]),
                "budget_withdrawn",
            ),
            (ArrasEventV2::AgentRevoked(vec![b]), "agent_revoked"),
            (
                ArrasEventV2::BudgetCapsSet(vec![BudgetCapsData {
                    owner: acc("human.testnet"),
                    agent: acc("agent.testnet"),
                    operator_id: "em".into(),
                    tope_por_pago: U128(1),
                    tope_diario: U128(2),
                }]),
                "budget_caps_set",
            ),
            (
                ArrasEventV2::OperatorUpdated(vec![OperatorDataV2 {
                    operator_id: "em".into(),
                    quitar_ticket_pubkey: Some(true),
                    ..Default::default()
                }]),
                "operator_updated",
            ),
        ];
        for (e, nombre) in casos {
            let log = event_log_v2(&e);
            assert!(log.starts_with(&format!("EVENT_JSON:{{\"standard\":\"arras\",\"version\":\"1.0.0\",\"event\":\"{nombre}\",\"data\":[{{")), "{log}");
        }
        // operator_updated de v2 lleva solo lo que cambio, igual que el de v1.
        let upd = event_log_v2(&ArrasEventV2::OperatorUpdated(vec![OperatorDataV2 {
            operator_id: "em".into(),
            arbiter_window_ns: Some(U64(60_000_000_000)),
            ..Default::default()
        }]));
        assert!(
            upd.ends_with(r#""data":[{"operator_id":"em","arbiter_window_ns":"60000000000"}]}"#),
            "{upd}"
        );
    }
}
