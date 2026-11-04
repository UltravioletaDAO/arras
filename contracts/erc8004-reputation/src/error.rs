//! Codigos de error del Reputation Registry. Panic: `ERC8004_REP_E04: too many decimals` (la tx falla y el runtime
//! devuelve el deposito). Rechazo de un callback (nunca entra en panico): log `erc8004:rechazo:<CODIGO>`, el deposito
//! entero de vuelta y `null`.

use near_sdk::env;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReputationError {
    E01AgentIdInvalido,
    E02ValueInvalido,
    E03ValueFueraDeRango,
    E04DemasiadosDecimales,
    E05TagInvalido,
    E06EndpointInvalido,
    E07FeedbackUriInvalida,
    E08HashInvalido,
    E09DepositoInsuficiente(u128),
    E10GasInsuficiente,
    E11FeedbackIndexInvalido,
    E12IndexCero,
    E13IndexFueraDeRango,
    E14YaRevocada,
    E15RequiereDeposito,
    E16ResponseUriInvalida,
    E17IdentityRegistryInvalido,
    E20VistaDeIdentidadFallo,
    E21RespuestaDeIdentidadInvalida,
    E22AgenteInexistente,
    E23AutoCalificacion,
    E30ClientesRequeridos,
    E40ProofInvalida,
    E41VerifierInvalido,
    E42PruebaYaUsada,
    E43PruebaEnVuelo,
    E44Reemplazado,
    E45VerificadorFallo,
    E46RespuestaDelVerificadorInvalida,
    E47NoEsElAgente,
    E48AgentWalletCalifica,
}

impl ReputationError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::E01AgentIdInvalido => "E01_AGENT_ID_INVALIDO",
            Self::E02ValueInvalido => "E02_VALUE_INVALIDO",
            Self::E03ValueFueraDeRango => "E03_VALUE_FUERA_DE_RANGO",
            Self::E04DemasiadosDecimales => "E04_DEMASIADOS_DECIMALES",
            Self::E05TagInvalido => "E05_TAG_INVALIDO",
            Self::E06EndpointInvalido => "E06_ENDPOINT_INVALIDO",
            Self::E07FeedbackUriInvalida => "E07_FEEDBACK_URI_INVALIDA",
            Self::E08HashInvalido => "E08_HASH_INVALIDO",
            Self::E09DepositoInsuficiente(_) => "E09_DEPOSITO_INSUFICIENTE",
            Self::E10GasInsuficiente => "E10_GAS_INSUFICIENTE",
            Self::E11FeedbackIndexInvalido => "E11_FEEDBACK_INDEX_INVALIDO",
            Self::E12IndexCero => "E12_INDEX_CERO",
            Self::E13IndexFueraDeRango => "E13_INDEX_FUERA_DE_RANGO",
            Self::E14YaRevocada => "E14_YA_REVOCADA",
            Self::E15RequiereDeposito => "E15_REQUIERE_DEPOSITO",
            Self::E16ResponseUriInvalida => "E16_RESPONSE_URI_INVALIDA",
            Self::E17IdentityRegistryInvalido => "E17_IDENTITY_REGISTRY_INVALIDO",
            Self::E20VistaDeIdentidadFallo => "E20_VISTA_DE_IDENTIDAD_FALLO",
            Self::E21RespuestaDeIdentidadInvalida => "E21_RESPUESTA_DE_IDENTIDAD_INVALIDA",
            Self::E22AgenteInexistente => "E22_AGENTE_INEXISTENTE",
            Self::E23AutoCalificacion => "E23_AUTO_CALIFICACION",
            Self::E30ClientesRequeridos => "E30_CLIENTES_REQUERIDOS",
            Self::E40ProofInvalida => "E40_PROOF_INVALIDA",
            Self::E41VerifierInvalido => "E41_VERIFIER_INVALIDO",
            Self::E42PruebaYaUsada => "E42_PRUEBA_YA_USADA",
            Self::E43PruebaEnVuelo => "E43_PRUEBA_EN_VUELO",
            Self::E44Reemplazado => "E44_REEMPLAZADO",
            Self::E45VerificadorFallo => "E45_VERIFICADOR_FALLO",
            Self::E46RespuestaDelVerificadorInvalida => "E46_RESPUESTA_DEL_VERIFICADOR_INVALIDA",
            Self::E47NoEsElAgente => "E47_NO_ES_EL_AGENTE",
            Self::E48AgentWalletCalifica => "E48_AGENT_WALLET_CALIFICA",
        }
    }

    pub fn number(&self) -> &'static str {
        &self.code()[..3]
    }

    pub fn message(&self) -> String {
        match self {
            Self::E01AgentIdInvalido => "agent_id invalido: decimal canonico de u64".into(),
            Self::E02ValueInvalido => {
                "value invalido: entero decimal canonico con signo opcional".into()
            }
            Self::E03ValueFueraDeRango => "value too large".into(),
            Self::E04DemasiadosDecimales => "too many decimals".into(),
            Self::E05TagInvalido => "tag invalido".into(),
            Self::E06EndpointInvalido => "endpoint invalido".into(),
            Self::E07FeedbackUriInvalida => "feedback_uri invalida".into(),
            Self::E08HashInvalido => "hash invalido: 64 hex en minusculas, sin 0x".into(),
            Self::E09DepositoInsuficiente(n) => {
                format!("deposito insuficiente para el storage: se necesitan {n} yocto")
            }
            Self::E10GasInsuficiente => "gas insuficiente para la llamada cruzada".into(),
            Self::E11FeedbackIndexInvalido => {
                "feedback_index invalido: decimal canonico de u64".into()
            }
            Self::E12IndexCero => "index must be > 0".into(),
            Self::E13IndexFueraDeRango => "index out of bounds".into(),
            Self::E14YaRevocada => "Already revoked".into(),
            Self::E15RequiereDeposito => "requiere al menos 1 yocto adjunto".into(),
            Self::E16ResponseUriInvalida => "response_uri invalida (Empty URI)".into(),
            Self::E17IdentityRegistryInvalido => "identity_registry invalido".into(),
            Self::E20VistaDeIdentidadFallo => "check_agent del Identity Registry fallo".into(),
            Self::E21RespuestaDeIdentidadInvalida => "respuesta de check_agent invalida".into(),
            Self::E22AgenteInexistente => "agente inexistente".into(),
            Self::E23AutoCalificacion => "Self-feedback not allowed".into(),
            Self::E30ClientesRequeridos => "clientAddresses required".into(),
            Self::E40ProofInvalida => "proof invalida".into(),
            Self::E41VerifierInvalido => "verifier invalido".into(),
            Self::E42PruebaYaUsada => "ya califico con esta prueba".into(),
            Self::E43PruebaEnVuelo => "hay una calificacion con esta prueba en vuelo".into(),
            Self::E44Reemplazado => "otra calificacion con la misma prueba reemplazo a esta".into(),
            Self::E45VerificadorFallo => "verify_feedback_proof del verificador fallo".into(),
            Self::E46RespuestaDelVerificadorInvalida => "respuesta del verificador invalida".into(),
            Self::E47NoEsElAgente => {
                "la otra parte de la prueba no es el dueno ni la agentWallet del agente".into()
            }
            Self::E48AgentWalletCalifica => "la agentWallet del agente no puede calificarlo".into(),
        }
    }

    pub fn panic_message(&self) -> String {
        format!("ERC8004_REP_{}: {}", self.number(), self.message())
    }

    pub fn panic(&self) -> ! {
        env::panic_str(&self.panic_message())
    }

    pub fn rejection_log(&self) -> String {
        format!("erc8004:rechazo:{}", self.code())
    }
}
