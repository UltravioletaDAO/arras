//! Codigos de error del Identity Registry. Formato del panic: `ERC8004_ID_E11: agente inexistente`.

use near_sdk::env;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityError {
    E01AgentUriInvalida,
    E02ClaveInvalida,
    E03DemasiadaMetadata,
    E04ClaveReservada,
    E05ValorLargo,
    E06DepositoInsuficiente(u128),
    E07RequiereDeposito,
    E10AgentIdInvalido,
    E11AgenteInexistente,
    E12NoAutorizado,
    E13DeadlineInvalido,
    E14DeadlineVencido,
    E15DeadlineLejos,
    E16SinWalletPendiente,
    E17NoEsLaWalletPendiente,
    E18MemoLargo,
    E19ApprovalIdNoCoincide,
    E20ReceptorEsElDueno,
    E21GasInsuficiente,
    E22SoloElDueno,
    E23DemasiadasAprobaciones,
}

impl IdentityError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::E01AgentUriInvalida => "E01_AGENT_URI_INVALIDA",
            Self::E02ClaveInvalida => "E02_CLAVE_INVALIDA",
            Self::E03DemasiadaMetadata => "E03_DEMASIADA_METADATA",
            Self::E04ClaveReservada => "E04_CLAVE_RESERVADA",
            Self::E05ValorLargo => "E05_VALOR_LARGO",
            Self::E06DepositoInsuficiente(_) => "E06_DEPOSITO_INSUFICIENTE",
            Self::E07RequiereDeposito => "E07_REQUIERE_DEPOSITO",
            Self::E10AgentIdInvalido => "E10_AGENT_ID_INVALIDO",
            Self::E11AgenteInexistente => "E11_AGENTE_INEXISTENTE",
            Self::E12NoAutorizado => "E12_NO_AUTORIZADO",
            Self::E13DeadlineInvalido => "E13_DEADLINE_INVALIDO",
            Self::E14DeadlineVencido => "E14_DEADLINE_VENCIDO",
            Self::E15DeadlineLejos => "E15_DEADLINE_LEJOS",
            Self::E16SinWalletPendiente => "E16_SIN_WALLET_PENDIENTE",
            Self::E17NoEsLaWalletPendiente => "E17_NO_ES_LA_WALLET_PENDIENTE",
            Self::E18MemoLargo => "E18_MEMO_LARGO",
            Self::E19ApprovalIdNoCoincide => "E19_APPROVAL_ID_NO_COINCIDE",
            Self::E20ReceptorEsElDueno => "E20_RECEPTOR_ES_EL_DUENO",
            Self::E21GasInsuficiente => "E21_GAS_INSUFICIENTE",
            Self::E22SoloElDueno => "E22_SOLO_EL_DUENO",
            Self::E23DemasiadasAprobaciones => "E23_DEMASIADAS_APROBACIONES",
        }
    }

    pub fn number(&self) -> &'static str {
        &self.code()[..3]
    }

    pub fn message(&self) -> String {
        match self {
            Self::E01AgentUriInvalida => "agent_uri invalida".into(),
            Self::E02ClaveInvalida => "clave de metadata invalida".into(),
            Self::E03DemasiadaMetadata => "demasiadas entradas de metadata".into(),
            Self::E04ClaveReservada => "reserved key: agentWallet".into(),
            Self::E05ValorLargo => "valor de metadata demasiado largo".into(),
            Self::E06DepositoInsuficiente(n) => {
                format!("deposito insuficiente para el storage: se necesitan {n} yocto")
            }
            Self::E07RequiereDeposito => "requiere al menos 1 yocto adjunto".into(),
            Self::E10AgentIdInvalido => "agent_id invalido: decimal canonico de u64".into(),
            Self::E11AgenteInexistente => "agente inexistente".into(),
            Self::E12NoAutorizado => "Not authorized".into(),
            Self::E13DeadlineInvalido => "deadline invalido: decimal canonico de u64 (ns)".into(),
            Self::E14DeadlineVencido => "expired".into(),
            Self::E15DeadlineLejos => "deadline too far".into(),
            Self::E16SinWalletPendiente => "no hay agentWallet pendiente".into(),
            Self::E17NoEsLaWalletPendiente => "quien llama no es la agentWallet pendiente".into(),
            Self::E18MemoLargo => "memo demasiado largo".into(),
            Self::E19ApprovalIdNoCoincide => "approval_id no coincide".into(),
            Self::E20ReceptorEsElDueno => "el receptor ya es el dueno".into(),
            Self::E21GasInsuficiente => "gas insuficiente para nft_transfer_call".into(),
            Self::E22SoloElDueno => "solo el dueno del agente".into(),
            Self::E23DemasiadasAprobaciones => "demasiadas aprobaciones para este agente".into(),
        }
    }

    pub fn panic_message(&self) -> String {
        format!("ERC8004_ID_{}: {}", self.number(), self.message())
    }

    pub fn panic(&self) -> ! {
        env::panic_str(&self.panic_message())
    }
}
