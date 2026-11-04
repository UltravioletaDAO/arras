//! Identidad: `register` y `update`. La identidad es la cuenta que llama; no hay ids ni NFT.
//!
//! Storage: se mide con `env::storage_usage()` antes y despues de escribir (con los caches persistidos). `register`
//! cobra lo que la ficha ocupa y devuelve el resto del deposito; `update` cobra lo que la ficha crece, o devuelve lo
//! que se achica (mas el deposito). Un deposito corto es `E05` (panic: el runtime devuelve el deposito y no queda
//! nada escrito).

use crate::events::AgentData;
use crate::validate::{check_agent_uri, check_metadata};
use crate::*;

#[near]
impl Registry {
    /// Da de alta a la cuenta que llama. `E01` -> `E02` -> `E03` (ya registrada) -> `E05` (deposito corto).
    #[payable]
    pub fn register(&mut self, agent_uri: String, metadata: Option<String>) -> AgentView {
        check_agent_uri(&agent_uri).unwrap_or_else(|e| e.panic());
        check_metadata(metadata.as_deref()).unwrap_or_else(|e| e.panic());
        let account = env::predecessor_account_id();
        if self.agents.contains_key(&account) {
            RegistryError::E03YaRegistrado.panic();
        }
        let now = env::block_timestamp();
        let agent = Agent {
            agent_uri,
            metadata,
            registered_at: now,
            updated_at: now,
        };
        let (cost, freed) = self.write_agent(&account, agent.clone());
        debug_assert_eq!(freed, 0);
        let deposit = env::attached_deposit().as_yoctonear();
        if deposit < cost {
            RegistryError::E05DepositoInsuficiente(cost).panic();
        }
        refund(&account, deposit - cost);
        self.emit_agent(true, &account, &agent);
        AgentView::of(account, &agent)
    }

    /// Reemplaza la ficha de la cuenta que llama (las dos partes: `metadata = null` la borra). Exige al menos 1 yocto
    /// (una llave FunctionCall no cambia la ficha). `E06` -> `E01` -> `E02` -> `E04` (no registrada) -> `E05`.
    #[payable]
    pub fn update(&mut self, agent_uri: String, metadata: Option<String>) -> AgentView {
        let deposit = env::attached_deposit().as_yoctonear();
        if deposit == 0 {
            RegistryError::E06RequiereDeposito.panic();
        }
        check_agent_uri(&agent_uri).unwrap_or_else(|e| e.panic());
        check_metadata(metadata.as_deref()).unwrap_or_else(|e| e.panic());
        let account = env::predecessor_account_id();
        let Some(old) = self.agents.get(&account).cloned() else {
            RegistryError::E04NoRegistrado.panic();
        };
        let agent = Agent {
            agent_uri,
            metadata,
            registered_at: old.registered_at,
            updated_at: env::block_timestamp(),
        };
        let (cost, freed) = self.write_agent(&account, agent.clone());
        if deposit < cost {
            RegistryError::E05DepositoInsuficiente(cost).panic();
        }
        refund(&account, deposit - cost + freed);
        self.emit_agent(false, &account, &agent);
        AgentView::of(account, &agent)
    }
}

impl Registry {
    /// Escribe la ficha y devuelve `(costo de lo que crecio, valor de lo que se libero)` en yocto.
    fn write_agent(&mut self, account: &AccountId, agent: Agent) -> (u128, u128) {
        self.flush_all();
        let before = env::storage_usage();
        self.agents.insert(account.clone(), agent);
        self.flush_all();
        let after = env::storage_usage();
        (
            bytes_cost(after.saturating_sub(before)),
            bytes_cost(before.saturating_sub(after)),
        )
    }

    fn emit_agent(&self, alta: bool, account: &AccountId, agent: &Agent) {
        let data = vec![AgentData {
            account_id: account.clone(),
            agent_uri: agent.agent_uri.clone(),
            metadata: agent.metadata.clone(),
            at: U64(agent.updated_at),
        }];
        if alta {
            RegistryEvent::AgentRegistered(data).emit();
        } else {
            RegistryEvent::AgentUpdated(data).emit();
        }
    }
}
