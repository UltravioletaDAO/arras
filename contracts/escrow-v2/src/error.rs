//! Errores de v2 (prescripcion sec. 4). Viven en este crate y no en `arras-escrow-core`: tocar el core cambia la wasm
//! de v1 aunque v1 no use nada nuevo (medido en el PR). Mismo formato que v1: panic `ARRAS_Enn: <mensaje>` y, en `ft_on_transfer`,
//! el log `arras:rechazo:<CODIGO>`. Los codigos de v1 pasan tal cual ([`ArrasErrorV2::V1`]); `E11` cambia solo el
//! texto del panic, porque v2 suma `dispute_deadline` a los plazos.
//!
//! Numeros nuevos: `E47`, `E48` (administracion), `E50`..`E56` (presupuesto y ticket), `E60`..`E66` (disputa). `E07` y
//! `E31` siguen retirados.

use arras_escrow_core::ArrasError;
use near_sdk::env;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrasErrorV2 {
    /// Un codigo de v1 (`E01`..`E46`).
    V1(ArrasError),
    E47VentanaArbitro,
    E48TicketPubkey,
    E50SinPresupuesto,
    E51TopeDiario,
    E52TopePorPagoPresupuesto,
    E53TicketInvalido,
    E54TicketVencido,
    E55AgenteInvalido,
    E56TopesPresupuesto,
    E60NoEsParte,
    E61FueraDePlazoDisputa,
    E62NoEsArbitro,
    E63RepartoInvalido,
    E64ArbitroEnPlazo,
    E65ArbitroVencido,
    E66ReasonHash,
}

impl From<ArrasError> for ArrasErrorV2 {
    fn from(e: ArrasError) -> Self {
        Self::V1(e)
    }
}

impl ArrasErrorV2 {
    /// El codigo con su nombre (p. ej. `E53_TICKET_INVALIDO`).
    pub const fn code(&self) -> &'static str {
        match self {
            Self::V1(e) => e.code(),
            Self::E47VentanaArbitro => "E47_VENTANA_ARBITRO",
            Self::E48TicketPubkey => "E48_TICKET_PUBKEY",
            Self::E50SinPresupuesto => "E50_SIN_PRESUPUESTO",
            Self::E51TopeDiario => "E51_TOPE_DIARIO",
            Self::E52TopePorPagoPresupuesto => "E52_TOPE_POR_PAGO_PRESUPUESTO",
            Self::E53TicketInvalido => "E53_TICKET_INVALIDO",
            Self::E54TicketVencido => "E54_TICKET_VENCIDO",
            Self::E55AgenteInvalido => "E55_AGENTE_INVALIDO",
            Self::E56TopesPresupuesto => "E56_TOPES_PRESUPUESTO",
            Self::E60NoEsParte => "E60_NO_ES_PARTE",
            Self::E61FueraDePlazoDisputa => "E61_FUERA_DE_PLAZO_DISPUTA",
            Self::E62NoEsArbitro => "E62_NO_ES_ARBITRO",
            Self::E63RepartoInvalido => "E63_REPARTO_INVALIDO",
            Self::E64ArbitroEnPlazo => "E64_ARBITRO_EN_PLAZO",
            Self::E65ArbitroVencido => "E65_ARBITRO_VENCIDO",
            Self::E66ReasonHash => "E66_REASON_HASH",
        }
    }

    /// El numero, `Enn`.
    pub fn number(&self) -> &'static str {
        &self.code()[..3]
    }

    /// El texto del panic, sin el prefijo.
    pub fn message(&self) -> String {
        match self {
            Self::V1(ArrasError::E11Plazos) => "plazos invalidos: se exige now < release_deadline < reclaim_after y now < dispute_deadline <= reclaim_after".into(),
            Self::V1(e) => e.message(),
            Self::E47VentanaArbitro => "arbiter_window_ns fuera de [1 minuto, 30 dias]".into(),
            Self::E48TicketPubkey => "ticket_pubkey invalida: solo ed25519".into(),
            Self::E50SinPresupuesto => "sin presupuesto (o saldo insuficiente) para este agente".into(),
            Self::E51TopeDiario => "supera el tope diario del presupuesto".into(),
            Self::E52TopePorPagoPresupuesto => "supera el tope por pago del presupuesto".into(),
            Self::E53TicketInvalido => "operator_ticket invalido o ausente".into(),
            Self::E54TicketVencido => "operator_ticket vencido".into(),
            Self::E55AgenteInvalido => "agente invalido".into(),
            Self::E56TopesPresupuesto => "topes del presupuesto invalidos (tienen que ser > 0)".into(),
            Self::E60NoEsParte => "solo el payer o el receiver".into(),
            Self::E61FueraDePlazoDisputa => "dispute_deadline vencido".into(),
            Self::E62NoEsArbitro => "solo el arbitro del operador".into(),
            Self::E63RepartoInvalido => "to_receiver + to_payer tiene que ser lo vivo del pago".into(),
            Self::E64ArbitroEnPlazo => "el arbitro todavia esta en plazo".into(),
            Self::E65ArbitroVencido => "vencio el plazo del arbitro".into(),
            Self::E66ReasonHash => "reason_hash invalido: hex de 64 en minusculas".into(),
        }
    }

    /// `ARRAS_E53: operator_ticket invalido o ausente`.
    pub fn panic_message(&self) -> String {
        format!("ARRAS_{}: {}", self.number(), self.message())
    }

    /// Entra en panico con [`Self::panic_message`].
    pub fn panic(&self) -> ! {
        env::panic_str(&self.panic_message())
    }

    /// `arras:rechazo:E53_TICKET_INVALIDO`, el log de un rechazo de `ft_on_transfer`.
    pub fn rejection_log(&self) -> String {
        format!("arras:rechazo:{}", self.code())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NUEVOS: [ArrasErrorV2; 16] = [
        ArrasErrorV2::E47VentanaArbitro,
        ArrasErrorV2::E48TicketPubkey,
        ArrasErrorV2::E50SinPresupuesto,
        ArrasErrorV2::E51TopeDiario,
        ArrasErrorV2::E52TopePorPagoPresupuesto,
        ArrasErrorV2::E53TicketInvalido,
        ArrasErrorV2::E54TicketVencido,
        ArrasErrorV2::E55AgenteInvalido,
        ArrasErrorV2::E56TopesPresupuesto,
        ArrasErrorV2::E60NoEsParte,
        ArrasErrorV2::E61FueraDePlazoDisputa,
        ArrasErrorV2::E62NoEsArbitro,
        ArrasErrorV2::E63RepartoInvalido,
        ArrasErrorV2::E64ArbitroEnPlazo,
        ArrasErrorV2::E65ArbitroVencido,
        ArrasErrorV2::E66ReasonHash,
    ];

    #[test]
    fn formato_igual_a_v1() {
        assert_eq!(
            ArrasErrorV2::E53TicketInvalido.panic_message(),
            "ARRAS_E53: operator_ticket invalido o ausente"
        );
        assert_eq!(
            ArrasErrorV2::E54TicketVencido.rejection_log(),
            "arras:rechazo:E54_TICKET_VENCIDO"
        );
        let v1: ArrasErrorV2 = ArrasError::E22Estado("release").into();
        assert_eq!(
            v1.panic_message(),
            "ARRAS_E22: estado invalido para release"
        );
        assert_eq!(
            v1.rejection_log(),
            ArrasError::E22Estado("release").rejection_log()
        );
        let e11: ArrasErrorV2 = ArrasError::E11Plazos.into();
        assert_eq!(e11.code(), "E11_PLAZOS");
        assert!(e11.message().contains("dispute_deadline"));
    }

    #[test]
    fn numeros_nuevos_unicos_y_sin_choque_con_v1() {
        let mut numeros: Vec<&str> = NUEVOS.iter().map(|e| e.number()).collect();
        numeros.sort_unstable();
        numeros.dedup();
        assert_eq!(numeros.len(), NUEVOS.len());
        for n in &numeros {
            let v: u32 = n[1..].parse().unwrap();
            assert!(v >= 47, "{n} no pisa un numero de v1 (E01..E46)");
            assert!(*n != "E07" && *n != "E31");
        }
        for e in NUEVOS {
            assert!(
                e.code().starts_with(e.number()) && e.code().as_bytes()[3] == b'_',
                "{e:?}"
            );
        }
    }
}
