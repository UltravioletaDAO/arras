//! Receptor de NFT de mentira para los tests near-workspaces de `contracts/erc8004-identity`. **Solo para el sandbox.**
//! `nft_on_transfer` (NEP-171) devuelve `true` (que le devuelvan el token) si `msg == "return"`, entra en panico si
//! `msg == "panic"`, y si no, `false` (se lo queda).

use near_sdk::{AccountId, env, near};

#[near(contract_state)]
#[derive(Default)]
pub struct TestNftReceiver {}

#[near]
impl TestNftReceiver {
    pub fn nft_on_transfer(
        &mut self,
        sender_id: AccountId,
        previous_owner_id: AccountId,
        token_id: String,
        msg: String,
    ) -> bool {
        env::log_str(&format!(
            "nft_on_transfer {sender_id} {previous_owner_id} {token_id} {msg}"
        ));
        if msg == "panic" {
            env::panic_str("el receptor entro en panico");
        }
        msg == "return"
    }
}
