//! Storage staking: quien llama paga lo que crece con el deposito adjunto; lo que sobra vuelve.

use near_sdk::{AccountId, NearToken, Promise, env};

/// Bytes que el protocolo suma por cada registro de storage (`num_extra_bytes_record`).
pub const RECORD_OVERHEAD_BYTES: u64 = 40;

/// Bytes de un registro de storage: overhead + clave + valor.
pub const fn record_bytes(key_len: u64, value_len: u64) -> u64 {
    RECORD_OVERHEAD_BYTES + key_len + value_len
}

/// borsh de un `String` de `n` bytes (u32 del largo + bytes).
pub const fn string_bytes(n: usize) -> u64 {
    4 + n as u64
}

/// borsh de un `AccountId` de largo maximo.
pub const ACCOUNT_MAX_BYTES: u64 = string_bytes(crate::MAX_ACCOUNT_ID_LEN);

/// Costo en yocto de `bytes` de storage (sin desbordar: `u64::MAX * 10^19` entra en u128).
pub fn bytes_cost(bytes: u64) -> u128 {
    u128::from(bytes).saturating_mul(env::storage_byte_cost().as_yoctonear())
}

/// Manda `amount` yocto a `to`, si hay algo que mandar. La promesa queda suelta: si `to` no existe, el runtime le
/// devuelve el monto al contrato.
pub fn refund(to: &AccountId, amount: u128) {
    if amount > 0 {
        Promise::new(to.clone())
            .transfer(NearToken::from_yoctonear(amount))
            .detach();
    }
}
