//! Escrow de mentira para los tests near-workspaces de `contracts/registry-v1`. **Solo para el sandbox.**
//!
//! `get_payment(payment_id)` devuelve, byte por byte, el JSON que se le cargo con `set_raw` (asi un test arma
//! respuestas que un escrow de Arras real no da: un pago con payer == receiver, un JSON roto, una respuesta de mas de
//! 8 KiB o una que no es exactamente un `Payment` de v1). Sin carga devuelve `null`; con el id `panic`, entra en panico.

use near_sdk::store::LookupMap;
use near_sdk::{PanicOnDefault, env, near};

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct TestEscrow {
    raw: LookupMap<String, String>,
}

#[near]
impl TestEscrow {
    #[init]
    pub fn new() -> Self {
        Self {
            raw: LookupMap::new(b"r"),
        }
    }

    /// Carga la respuesta cruda de `get_payment(payment_id)`. Cualquiera (es un doble de test).
    pub fn set_raw(&mut self, payment_id: String, raw: String) {
        self.raw.insert(payment_id, raw);
    }

    /// La respuesta cargada tal cual (sin serializar de nuevo); `null` si no hay; panic con `panic`.
    pub fn get_payment(&self, payment_id: String) {
        if payment_id == "panic" {
            env::panic_str("test-escrow: panic pedido");
        }
        match self.raw.get(&payment_id) {
            Some(r) => env::value_return(r.as_bytes()),
            None => env::value_return(b"null"),
        }
    }
}
