//! Codigos de error del registro. Formato del panic: `ARRAS_REG_E10: escrow no permitido`.
//!
//! Las guardas de `give_feedback` que se pueden ver antes de la llamada cruzada entran en panico (la transaccion
//! falla y el runtime devuelve el deposito). Las del callback (`on_payment_checked`, `E20`..`E26` y `E18` otra vez)
//! **nunca** entran en panico: devuelven el deposito entero, dejan el log `arras-registry:rechazo:<CODIGO>` y el
//! callback devuelve `null`.

use near_sdk::env;

/// Un error del registro. Los que llevan `u128` dicen cuanto deposito hacia falta (yocto).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistryError {
    E01AgentUriInvalida,
    E02MetadataInvalida,
    E03YaRegistrado,
    E04NoRegistrado,
    E05DepositoInsuficiente(u128),
    E06RequiereDeposito,
    E10EscrowNoPermitido,
    E11PaymentIdInvalido,
    E12ScoreInvalido,
    E13TagInvalido,
    E14FeedbackUriInvalida,
    E15FeedbackHashInvalido,
    E16DepositoFeedback(u128),
    E17GasInsuficiente,
    E18YaCalificado,
    E19EnVuelo,
    E20GetPaymentFallo,
    E21RespuestaInvalida,
    E22PagoInexistente,
    E23PagoSinLiberar,
    E24NoEsParte,
    E25AutoCalificacion,
    E26Reemplazado,
    E30FeedbackInexistente,
    E31YaRevocado,
    E40EscrowsInvalidos,
}

impl RegistryError {
    /// El codigo con su nombre (p. ej. `E22_PAGO_INEXISTENTE`).
    pub const fn code(&self) -> &'static str {
        match self {
            Self::E01AgentUriInvalida => "E01_AGENT_URI_INVALIDA",
            Self::E02MetadataInvalida => "E02_METADATA_INVALIDA",
            Self::E03YaRegistrado => "E03_YA_REGISTRADO",
            Self::E04NoRegistrado => "E04_NO_REGISTRADO",
            Self::E05DepositoInsuficiente(_) => "E05_DEPOSITO_INSUFICIENTE",
            Self::E06RequiereDeposito => "E06_REQUIERE_DEPOSITO",
            Self::E10EscrowNoPermitido => "E10_ESCROW_NO_PERMITIDO",
            Self::E11PaymentIdInvalido => "E11_PAYMENT_ID_INVALIDO",
            Self::E12ScoreInvalido => "E12_SCORE_INVALIDO",
            Self::E13TagInvalido => "E13_TAG_INVALIDO",
            Self::E14FeedbackUriInvalida => "E14_FEEDBACK_URI_INVALIDA",
            Self::E15FeedbackHashInvalido => "E15_FEEDBACK_HASH_INVALIDO",
            Self::E16DepositoFeedback(_) => "E16_DEPOSITO_FEEDBACK",
            Self::E17GasInsuficiente => "E17_GAS_INSUFICIENTE",
            Self::E18YaCalificado => "E18_YA_CALIFICADO",
            Self::E19EnVuelo => "E19_EN_VUELO",
            Self::E20GetPaymentFallo => "E20_GET_PAYMENT_FALLO",
            Self::E21RespuestaInvalida => "E21_RESPUESTA_INVALIDA",
            Self::E22PagoInexistente => "E22_PAGO_INEXISTENTE",
            Self::E23PagoSinLiberar => "E23_PAGO_SIN_LIBERAR",
            Self::E24NoEsParte => "E24_NO_ES_PARTE",
            Self::E25AutoCalificacion => "E25_AUTO_CALIFICACION",
            Self::E26Reemplazado => "E26_REEMPLAZADO",
            Self::E30FeedbackInexistente => "E30_FEEDBACK_INEXISTENTE",
            Self::E31YaRevocado => "E31_YA_REVOCADO",
            Self::E40EscrowsInvalidos => "E40_ESCROWS_INVALIDOS",
        }
    }

    /// El numero, `Enn`.
    pub fn number(&self) -> &'static str {
        &self.code()[..3]
    }

    /// El texto del panic, sin el prefijo.
    pub fn message(&self) -> String {
        match self {
            Self::E01AgentUriInvalida => "agent_uri invalida".into(),
            Self::E02MetadataInvalida => "metadata invalida".into(),
            Self::E03YaRegistrado => "la cuenta ya esta registrada (usar update)".into(),
            Self::E04NoRegistrado => "la cuenta no esta registrada".into(),
            Self::E05DepositoInsuficiente(n) => {
                format!("deposito insuficiente para el storage: se necesitan {n} yocto")
            }
            Self::E06RequiereDeposito => "requiere al menos 1 yocto adjunto".into(),
            Self::E10EscrowNoPermitido => "escrow no permitido".into(),
            Self::E11PaymentIdInvalido => "payment_id invalido".into(),
            Self::E12ScoreInvalido => "score fuera de 0..=100".into(),
            Self::E13TagInvalido => "tag invalido".into(),
            Self::E14FeedbackUriInvalida => "feedback_uri invalida".into(),
            Self::E15FeedbackHashInvalido => {
                "feedback_hash invalido: 64 hex en minusculas, sin 0x".into()
            }
            Self::E16DepositoFeedback(n) => {
                format!("deposito insuficiente para la calificacion: se necesitan {n} yocto")
            }
            Self::E17GasInsuficiente => "gas insuficiente para la llamada cruzada".into(),
            Self::E18YaCalificado => "ya califico este pago".into(),
            Self::E19EnVuelo => "hay una calificacion de este pago en vuelo".into(),
            Self::E20GetPaymentFallo => "get_payment del escrow fallo".into(),
            Self::E21RespuestaInvalida => "respuesta de get_payment invalida".into(),
            Self::E22PagoInexistente => "pago inexistente".into(),
            Self::E23PagoSinLiberar => {
                "el pago no esta Released ni PartiallyReleased con plata liberada".into()
            }
            Self::E24NoEsParte => "quien califica no es el payer ni el receiver".into(),
            Self::E25AutoCalificacion => "no se puede calificar a si mismo".into(),
            Self::E26Reemplazado => "otra calificacion del mismo pago reemplazo a esta".into(),
            Self::E30FeedbackInexistente => "calificacion inexistente".into(),
            Self::E31YaRevocado => "la calificacion ya esta revocada".into(),
            Self::E40EscrowsInvalidos => {
                "escrows: de 1 a 8 cuentas, sin repetir y sin el propio registro".into()
            }
        }
    }

    /// `ARRAS_REG_Enn: mensaje`.
    pub fn panic_message(&self) -> String {
        format!("ARRAS_REG_{}: {}", self.number(), self.message())
    }

    pub fn panic(&self) -> ! {
        env::panic_str(&self.panic_message())
    }

    /// El log de un rechazo del callback: `arras-registry:rechazo:<CODIGO>`.
    pub fn rejection_log(&self) -> String {
        format!("arras-registry:rechazo:{}", self.code())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formato_de_panic_y_de_rechazo() {
        let e = RegistryError::E22PagoInexistente;
        assert_eq!(e.number(), "E22");
        assert_eq!(e.panic_message(), "ARRAS_REG_E22: pago inexistente");
        assert_eq!(
            e.rejection_log(),
            "arras-registry:rechazo:E22_PAGO_INEXISTENTE"
        );
        assert_eq!(
            RegistryError::E16DepositoFeedback(7).panic_message(),
            "ARRAS_REG_E16: deposito insuficiente para la calificacion: se necesitan 7 yocto"
        );
    }

    #[test]
    fn codigos_unicos_y_con_numero() {
        let todos = [
            RegistryError::E01AgentUriInvalida,
            RegistryError::E02MetadataInvalida,
            RegistryError::E03YaRegistrado,
            RegistryError::E04NoRegistrado,
            RegistryError::E05DepositoInsuficiente(0),
            RegistryError::E06RequiereDeposito,
            RegistryError::E10EscrowNoPermitido,
            RegistryError::E11PaymentIdInvalido,
            RegistryError::E12ScoreInvalido,
            RegistryError::E13TagInvalido,
            RegistryError::E14FeedbackUriInvalida,
            RegistryError::E15FeedbackHashInvalido,
            RegistryError::E16DepositoFeedback(0),
            RegistryError::E17GasInsuficiente,
            RegistryError::E18YaCalificado,
            RegistryError::E19EnVuelo,
            RegistryError::E20GetPaymentFallo,
            RegistryError::E21RespuestaInvalida,
            RegistryError::E22PagoInexistente,
            RegistryError::E23PagoSinLiberar,
            RegistryError::E24NoEsParte,
            RegistryError::E25AutoCalificacion,
            RegistryError::E26Reemplazado,
            RegistryError::E30FeedbackInexistente,
            RegistryError::E31YaRevocado,
            RegistryError::E40EscrowsInvalidos,
        ];
        let mut vistos = std::collections::BTreeSet::new();
        for e in todos {
            assert!(vistos.insert(e.number()), "numero repetido: {}", e.code());
            assert!(e.code().starts_with('E') && e.code().as_bytes()[3] == b'_');
        }
    }
}
