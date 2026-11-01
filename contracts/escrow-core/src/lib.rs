//! Arras escrow core: lo que comparten v1 y v2 y que no es estado de contrato.
//!
//! - [`id`]: el `msg` canonico de `ft_transfer_call` y el `payment_id` (sec. 3.2 y 7 de la prescripcion).
//! - [`fees`]: `fee = floor(x * fee_bps / 10000)`, `net = x - fee` (sec. 3.8).
//! - [`error`]: los codigos `ARRAS_Enn` y el log de rechazo de `ft_on_transfer` (sec. 3.4).
//! - [`events`]: los eventos NEP-297 con `standard` `arras` (sec. 3.7).

pub mod error;
pub mod events;
pub mod fees;
pub mod id;

pub use error::ArrasError;
pub use fees::{FeeSplit, fee_split};
pub use id::{
    AuthorizeMsg, MAX_OPERATOR_ID_LEN, MSG_VERSION, PaymentId, PaymentIdInput, compute_payment_id,
    decode_payment_id, is_valid_operator_id, parse_authorize_msg, payment_id_preimage,
};

/// Tope de `fee_bps` de un operador: `FASE5_MAX_FEE_BPS` de Execution Market (`fees.py:33`).
pub const MAX_FEE_BPS: u16 = 1800;

/// Denominador de los basis points.
pub const BPS_DENOMINATOR: u128 = 10_000;
