//! Vistas: `get_config`, `get_agent`, `list_agents`, `get_feedback`, `get_feedback_for_payment`, `list_feedback`,
//! `get_summary` e `is_feedback_pending`. Las paginadas recortan `limit` a 50 (y 50 si no viene).

use crate::validate::hex;
use crate::*;

#[near(serializers = [json])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentView {
    pub account_id: AccountId,
    pub agent_uri: String,
    pub metadata: Option<String>,
    pub registered_at: U64,
    pub updated_at: U64,
}

impl AgentView {
    pub fn of(account_id: AccountId, a: &Agent) -> Self {
        Self {
            account_id,
            agent_uri: a.agent_uri.clone(),
            metadata: a.metadata.clone(),
            registered_at: U64(a.registered_at),
            updated_at: U64(a.updated_at),
        }
    }
}

#[near(serializers = [json])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeedbackView {
    pub feedback_id: u32,
    pub rater: AccountId,
    pub ratee: AccountId,
    pub ratee_role: Role,
    pub escrow: AccountId,
    pub payment_id: PaymentId,
    pub operator_id: String,
    pub released: U128,
    pub score: u8,
    pub tag1: Option<String>,
    pub tag2: Option<String>,
    pub feedback_uri: Option<String>,
    /// 64 hex en minusculas.
    pub feedback_hash: Option<String>,
    pub created_at: U64,
    pub revoked: bool,
    /// `null` si no esta revocada.
    pub revoked_at: Option<U64>,
}

impl FeedbackView {
    pub fn of(feedback_id: u32, f: &Feedback) -> Self {
        Self {
            feedback_id,
            rater: f.rater.clone(),
            ratee: f.ratee.clone(),
            ratee_role: f.ratee_role,
            escrow: f.escrow.clone(),
            payment_id: f.payment_id.clone(),
            operator_id: f.operator_id.clone(),
            released: U128(f.released),
            score: f.score,
            tag1: f.tag1.clone(),
            tag2: f.tag2.clone(),
            feedback_uri: f.feedback_uri.clone(),
            feedback_hash: f.feedback_hash.as_ref().map(hex),
            created_at: U64(f.created_at),
            revoked: f.revoked,
            revoked_at: f.revoked.then_some(U64(f.revoked_at)),
        }
    }
}

/// Contadores de un rol (o del total). `average_x100` = `floor(score_sum * 100 / count)` (8750 = 87,50); `null` sin
/// calificaciones vigentes.
#[near(serializers = [json])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoleSummaryView {
    pub count: u32,
    pub revoked: u32,
    pub score_sum: U64,
    pub average_x100: Option<u32>,
}

impl RoleSummaryView {
    fn of(s: RoleStats) -> Self {
        let average_x100 = (s.count > 0).then(|| {
            let avg = u128::from(s.score_sum) * 100 / u128::from(s.count);
            u32::try_from(avg).unwrap_or(u32::MAX)
        });
        Self {
            count: s.count,
            revoked: s.revoked,
            score_sum: U64(s.score_sum),
            average_x100,
        }
    }
}

/// Resumen de una cuenta: el total, como payer (la califico el receiver) y como receiver (la califico el payer), y la
/// ultima recibida (vigente o no).
#[near(serializers = [json])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SummaryView {
    pub account_id: AccountId,
    pub count: u32,
    pub revoked: u32,
    pub score_sum: U64,
    pub average_x100: Option<u32>,
    pub as_payer: RoleSummaryView,
    pub as_receiver: RoleSummaryView,
    pub last_feedback_id: Option<u32>,
    pub last_at: Option<U64>,
}

#[near(serializers = [json])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Limits {
    pub max_agent_uri_len: u32,
    pub max_metadata_len: u32,
    pub max_feedback_uri_len: u32,
    pub max_tag_len: u32,
    pub max_payment_id_len: u32,
    pub max_score: u8,
    pub max_list_limit: u32,
    pub pending_ttl_blocks: U64,
}

#[near(serializers = [json])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigView {
    /// `"arras-registry-v1"`.
    pub version: String,
    pub escrows: Vec<AccountId>,
    pub agents: u32,
    pub feedbacks: u32,
    /// Deposito que cubre el peor caso de `register` (se cobra lo medido y vuelve el resto).
    pub register_deposit: U128,
    /// Deposito que exige `give_feedback` (se cobra lo medido y vuelve el resto).
    pub feedback_deposit: U128,
    /// Gas minimo de `give_feedback`, en unidades de gas.
    pub min_gas_give_feedback: U64,
    pub limits: Limits,
}

fn page(from_index: Option<u32>, limit: Option<u32>, len: u32) -> std::ops::Range<u32> {
    let from = from_index.unwrap_or(0).min(len);
    let end = from
        .saturating_add(limit.unwrap_or(MAX_LIST_LIMIT).min(MAX_LIST_LIMIT))
        .min(len);
    from..end
}

#[near]
impl Registry {
    pub fn get_config(&self) -> ConfigView {
        ConfigView {
            version: VERSION.to_string(),
            escrows: self.escrows.clone(),
            agents: self.agents.len(),
            feedbacks: self.feedbacks.len(),
            register_deposit: U128(bytes_cost(AGENT_MAX_BYTES)),
            feedback_deposit: U128(Self::feedback_deposit()),
            min_gas_give_feedback: U64(MIN_GAS_GIVE_FEEDBACK.as_gas()),
            limits: Limits {
                max_agent_uri_len: MAX_AGENT_URI_LEN as u32,
                max_metadata_len: MAX_METADATA_LEN as u32,
                max_feedback_uri_len: MAX_FEEDBACK_URI_LEN as u32,
                max_tag_len: MAX_TAG_LEN as u32,
                max_payment_id_len: MAX_PAYMENT_ID_LEN as u32,
                max_score: MAX_SCORE,
                max_list_limit: MAX_LIST_LIMIT,
                pending_ttl_blocks: U64(PENDING_TTL_BLOCKS),
            },
        }
    }

    pub fn get_agent(&self, account_id: AccountId) -> Option<AgentView> {
        self.agents
            .get(&account_id)
            .map(|a| AgentView::of(account_id, a))
    }

    /// En orden de alta.
    pub fn list_agents(&self, from_index: Option<u32>, limit: Option<u32>) -> Vec<AgentView> {
        let r = page(from_index, limit, self.agents.len());
        self.agents
            .iter()
            .skip(r.start as usize)
            .take(r.len())
            .map(|(k, a)| AgentView::of(k.clone(), a))
            .collect()
    }

    pub fn get_feedback(&self, feedback_id: u32) -> Option<FeedbackView> {
        self.feedbacks
            .get(feedback_id)
            .map(|f| FeedbackView::of(feedback_id, f))
    }

    /// La calificacion de `rater` sobre `(escrow, payment_id)`, si existe.
    pub fn get_feedback_for_payment(
        &self,
        escrow: AccountId,
        payment_id: PaymentId,
        rater: AccountId,
    ) -> Option<FeedbackView> {
        let id = *self
            .feedback_by_key
            .get(&feedback_key(&escrow, &payment_id, &rater))?;
        self.get_feedback(id)
    }

    /// Las calificaciones que recibio `account_id`, en orden de llegada (las revocadas tambien, marcadas).
    pub fn list_feedback(
        &self,
        account_id: AccountId,
        from_index: Option<u32>,
        limit: Option<u32>,
    ) -> Vec<FeedbackView> {
        let Some(index) = self.received.get(&account_id) else {
            return vec![];
        };
        page(from_index, limit, index.len())
            .filter_map(|i| index.get(i))
            .filter_map(|&id| self.get_feedback(id))
            .collect()
    }

    pub fn get_summary(&self, account_id: AccountId) -> SummaryView {
        let s = self.summaries.get(&account_id).copied();
        let st = s.unwrap_or_default();
        let total = RoleStats {
            count: st.as_payer.count.saturating_add(st.as_receiver.count),
            revoked: st.as_payer.revoked.saturating_add(st.as_receiver.revoked),
            score_sum: st
                .as_payer
                .score_sum
                .saturating_add(st.as_receiver.score_sum),
        };
        let t = RoleSummaryView::of(total);
        SummaryView {
            account_id,
            count: t.count,
            revoked: t.revoked,
            score_sum: t.score_sum,
            average_x100: t.average_x100,
            as_payer: RoleSummaryView::of(st.as_payer),
            as_receiver: RoleSummaryView::of(st.as_receiver),
            last_feedback_id: s.map(|s| s.last_feedback_id),
            last_at: s.map(|s| U64(s.last_at)),
        }
    }

    /// `true` si hay un `give_feedback` de `rater` sobre `(escrow, payment_id)` en vuelo (candado sin vencer).
    pub fn is_feedback_pending(
        &self,
        escrow: AccountId,
        payment_id: PaymentId,
        rater: AccountId,
    ) -> bool {
        self.pending
            .get(&feedback_key(&escrow, &payment_id, &rater))
            .is_some_and(|&at| env::block_height() < at.saturating_add(PENDING_TTL_BLOCKS))
    }
}
