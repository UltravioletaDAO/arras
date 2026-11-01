//! Codigos de error (sec. 3.4). Formato de panic: `ARRAS_E22: estado invalido para release`.
//! `ft_on_transfer` nunca entra en panico: su rechazo es `Value(amount)` + el log `arras:rechazo:<CODIGO>`.
//!
//! `E07` (lista de payers) y `E31` (candado) quedan retirados y sus numeros no se reusan.

use near_sdk::env;

/// Un error de Arras. `E22_ESTADO` lleva el nombre de la operacion que lo rechaza.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrasError {
    E01TokenNoPermitido,
    E02Pausado,
    E03MsgInvalido,
    E04OperadorInexistente,
    E05DebajoDelMinimo,
    E06TopePorPago,
    E08TvlPayer,
    E09TvlOperador,
    E10TvlGlobal,
    E11Plazos,
    E12FeeMayorAlMaximo,
    E13IdUsado,
    E14SinStorage,
    E15ReceiverInvalido,
    E20PagoInexistente,
    E21NoEsCapturer,
    E22Estado(&'static str),
    E23PlazoVencido,
    E24Monto,
    E25NoAutorizado,
    E26NoEsPayer,
    E27TodaviaNo,
    E28Retencion,
    E30SinSaldo,
    E40SoloOwner,
    E41SoloGuardianUOwner,
    E42SoloBajarTopes,
    E43FeeBps,
    E44OperadorExiste,
    /// No esta en la tabla de la sec. 3.4: `register_operator` necesita rechazar un `operator_id` fuera de
    /// `[a-z0-9_-]{1,32}` (sec. 3.1). Anotado en PREGUNTA-c0der.md.
    E45OperatorIdInvalido,
    /// Tampoco esta en la sec. 3.4: un `fee_recipient` igual al propio contrato deja las tarifas atrapadas para
    /// siempre (`ft_transfer` de si mismo a si mismo falla). Refutacion de H-1, P3-1.
    E46FeeRecipientInvalido,
}

impl ArrasError {
    /// El codigo con su nombre, el de la tabla de la sec. 3.4 (p. ej. `E06_TOPE_POR_PAGO`).
    pub const fn code(&self) -> &'static str {
        match self {
            Self::E01TokenNoPermitido => "E01_TOKEN_NO_PERMITIDO",
            Self::E02Pausado => "E02_PAUSADO",
            Self::E03MsgInvalido => "E03_MSG_INVALIDO",
            Self::E04OperadorInexistente => "E04_OPERADOR_INEXISTENTE",
            Self::E05DebajoDelMinimo => "E05_DEBAJO_DEL_MINIMO",
            Self::E06TopePorPago => "E06_TOPE_POR_PAGO",
            Self::E08TvlPayer => "E08_TVL_PAYER",
            Self::E09TvlOperador => "E09_TVL_OPERADOR",
            Self::E10TvlGlobal => "E10_TVL_GLOBAL",
            Self::E11Plazos => "E11_PLAZOS",
            Self::E12FeeMayorAlMaximo => "E12_FEE_MAYOR_AL_MAXIMO",
            Self::E13IdUsado => "E13_ID_USADO",
            Self::E14SinStorage => "E14_SIN_STORAGE",
            Self::E15ReceiverInvalido => "E15_RECEIVER_INVALIDO",
            Self::E20PagoInexistente => "E20_PAGO_INEXISTENTE",
            Self::E21NoEsCapturer => "E21_NO_ES_CAPTURER",
            Self::E22Estado(_) => "E22_ESTADO",
            Self::E23PlazoVencido => "E23_PLAZO_VENCIDO",
            Self::E24Monto => "E24_MONTO",
            Self::E25NoAutorizado => "E25_NO_AUTORIZADO",
            Self::E26NoEsPayer => "E26_NO_ES_PAYER",
            Self::E27TodaviaNo => "E27_TODAVIA_NO",
            Self::E28Retencion => "E28_RETENCION",
            Self::E30SinSaldo => "E30_SIN_SALDO",
            Self::E40SoloOwner => "E40_SOLO_OWNER",
            Self::E41SoloGuardianUOwner => "E41_SOLO_GUARDIAN_U_OWNER",
            Self::E42SoloBajarTopes => "E42_SOLO_BAJAR_TOPES",
            Self::E43FeeBps => "E43_FEE_BPS",
            Self::E44OperadorExiste => "E44_OPERADOR_EXISTE",
            Self::E45OperatorIdInvalido => "E45_OPERATOR_ID_INVALIDO",
            Self::E46FeeRecipientInvalido => "E46_FEE_RECIPIENT_INVALIDO",
        }
    }

    /// El numero, `Enn`.
    pub fn number(&self) -> &'static str {
        &self.code()[..3]
    }

    /// El texto del panic, sin el prefijo.
    pub fn message(&self) -> String {
        match self {
            Self::E01TokenNoPermitido => "token no permitido".into(),
            Self::E02Pausado => "contrato en pausa".into(),
            Self::E03MsgInvalido => "msg invalido".into(),
            Self::E04OperadorInexistente => "operador inexistente".into(),
            Self::E05DebajoDelMinimo => "monto debajo del minimo del operador".into(),
            Self::E06TopePorPago => "monto mayor al tope por pago".into(),
            Self::E08TvlPayer => "supera el tope de TVL del payer".into(),
            Self::E09TvlOperador => "supera el tope de TVL del operador".into(),
            Self::E10TvlGlobal => "supera el tope de TVL global".into(),
            Self::E11Plazos => {
                "plazos invalidos: se exige now < release_deadline < reclaim_after".into()
            }
            Self::E12FeeMayorAlMaximo => "fee del operador mayor al fee_bps_max firmado".into(),
            Self::E13IdUsado => "payment_id ya usado".into(),
            Self::E14SinStorage => "sin storage NEP-145 del payer ni patrocinio disponible".into(),
            Self::E15ReceiverInvalido => "receiver invalido".into(),
            Self::E20PagoInexistente => "pago inexistente".into(),
            Self::E21NoEsCapturer => "no es capturer del operador".into(),
            Self::E22Estado(op) => format!("estado invalido para {op}"),
            Self::E23PlazoVencido => "release_deadline vencido".into(),
            Self::E24Monto => "monto invalido".into(),
            Self::E25NoAutorizado => "solo un capturer del operador o el receiver".into(),
            Self::E26NoEsPayer => "solo el payer".into(),
            Self::E27TodaviaNo => "todavia no vence reclaim_after".into(),
            Self::E28Retencion => "retencion de 7 dias sin cumplir".into(),
            Self::E30SinSaldo => "sin saldo para retirar".into(),
            Self::E40SoloOwner => "solo el owner".into(),
            Self::E41SoloGuardianUOwner => "solo el guardian o el owner".into(),
            Self::E42SoloBajarTopes => "los topes solo bajan".into(),
            Self::E43FeeBps => "fee_bps mayor a 1800".into(),
            Self::E44OperadorExiste => "el operador ya existe".into(),
            Self::E45OperatorIdInvalido => "operator_id fuera de [a-z0-9_-]{1,32}".into(),
            Self::E46FeeRecipientInvalido => "fee_recipient no puede ser el propio contrato".into(),
        }
    }

    /// `ARRAS_E22: estado invalido para release`.
    pub fn panic_message(&self) -> String {
        format!("ARRAS_{}: {}", self.number(), self.message())
    }

    /// Entra en panico con [`Self::panic_message`].
    pub fn panic(&self) -> ! {
        env::panic_str(&self.panic_message())
    }

    /// `arras:rechazo:E06_TOPE_POR_PAGO`, el log de un rechazo de `ft_on_transfer`.
    pub fn rejection_log(&self) -> String {
        format!("arras:rechazo:{}", self.code())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formato_de_panic_y_de_rechazo() {
        assert_eq!(
            ArrasError::E22Estado("release").panic_message(),
            "ARRAS_E22: estado invalido para release"
        );
        assert_eq!(
            ArrasError::E06TopePorPago.rejection_log(),
            "arras:rechazo:E06_TOPE_POR_PAGO"
        );
        assert_eq!(ArrasError::E41SoloGuardianUOwner.number(), "E41");
    }

    #[test]
    fn codigos_unicos_sin_e07_ni_e31() {
        let todos = [
            ArrasError::E01TokenNoPermitido,
            ArrasError::E02Pausado,
            ArrasError::E03MsgInvalido,
            ArrasError::E04OperadorInexistente,
            ArrasError::E05DebajoDelMinimo,
            ArrasError::E06TopePorPago,
            ArrasError::E08TvlPayer,
            ArrasError::E09TvlOperador,
            ArrasError::E10TvlGlobal,
            ArrasError::E11Plazos,
            ArrasError::E12FeeMayorAlMaximo,
            ArrasError::E13IdUsado,
            ArrasError::E14SinStorage,
            ArrasError::E15ReceiverInvalido,
            ArrasError::E20PagoInexistente,
            ArrasError::E21NoEsCapturer,
            ArrasError::E22Estado("x"),
            ArrasError::E23PlazoVencido,
            ArrasError::E24Monto,
            ArrasError::E25NoAutorizado,
            ArrasError::E26NoEsPayer,
            ArrasError::E27TodaviaNo,
            ArrasError::E28Retencion,
            ArrasError::E30SinSaldo,
            ArrasError::E40SoloOwner,
            ArrasError::E41SoloGuardianUOwner,
            ArrasError::E42SoloBajarTopes,
            ArrasError::E43FeeBps,
            ArrasError::E44OperadorExiste,
            ArrasError::E45OperatorIdInvalido,
            ArrasError::E46FeeRecipientInvalido,
        ];
        let mut numeros: Vec<&str> = todos.iter().map(|e| e.number()).collect();
        numeros.sort_unstable();
        numeros.dedup();
        assert_eq!(numeros.len(), todos.len());
        assert!(!numeros.contains(&"E07") && !numeros.contains(&"E31"));
        for e in todos {
            assert!(
                e.code().starts_with(e.number()) && e.code().as_bytes()[3] == b'_',
                "{e:?}"
            );
        }
    }
}
