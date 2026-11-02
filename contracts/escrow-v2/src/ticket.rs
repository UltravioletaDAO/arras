//! `operator_ticket` (prescripcion sec. 4; plan v2 sec. 3.3 y F-5): la firma ed25519 del operador que autoriza a un
//! payer a usar su espacio. Si el operador tiene `ticket_pubkey`, todo lo que entra en ese operador la necesita: cada
//! pago (por `ft_transfer_call` o por `authorize_from_budget`) y cada deposito de presupuesto. Asi nadie llena el TVL
//! ni vacia el `sponsor_pool` del operador con cuentas descartables (griefing del pool de storage).
//!
//! Mensaje firmado = `borsh(TicketPreimage)`:
//!
//! | Campo | Tipo borsh | Valor |
//! |---|---|---|
//! | `domain` | String | `arras:v2:operator_ticket` ([`TICKET_DOMAIN`]) |
//! | `contract` | String | la cuenta del contrato (`env::current_account_id()`) |
//! | `subject` | enum (tag u8) | `0` = pago: `payment_id` String; `1` = presupuesto: `operator_id`, `owner`, `agent` String |
//! | `expiry` | u64 LE | ns; el ticket vale mientras `now < expiry` (`E54` si no) |
//!
//! La prescripcion decia `borsh(payment_id, expiry)`; el dominio y la cuenta del contrato se suman para que un ticket
//! no sirva en otro contrato que confie en la misma llave (un v2 nuevo tras pausar este, o testnet contra mainnet) ni
//! como firma de otra cosa. Un ticket de pago es de un solo uso de hecho: el `payment_id` no se repite (`E13`).

use near_sdk::borsh::{self, BorshSerialize};
use near_sdk::{bs58, env};

use crate::msg::OperatorTicket;

/// Separador de dominio del mensaje del ticket.
pub const TICKET_DOMAIN: &str = "arras:v2:operator_ticket";

/// Lo que el ticket autoriza.
#[derive(BorshSerialize, Debug, Clone, Copy, PartialEq, Eq)]
#[borsh(crate = "near_sdk::borsh")]
pub enum TicketSubject<'a> {
    /// Un pago: el ticket vale para ese `payment_id` y nada mas.
    Payment { payment_id: &'a str },
    /// Un deposito de presupuesto: el operador aprueba el par `(owner, agent)` en su `operator_id` hasta `expiry`.
    Budget {
        operator_id: &'a str,
        owner: &'a str,
        agent: &'a str,
    },
}

#[derive(BorshSerialize)]
#[borsh(crate = "near_sdk::borsh")]
struct TicketPreimage<'a> {
    domain: &'a str,
    contract: &'a str,
    subject: TicketSubject<'a>,
    expiry: u64,
}

/// Los bytes que firma el operador.
pub fn ticket_message(contract: &str, subject: TicketSubject<'_>, expiry: u64) -> Vec<u8> {
    borsh::to_vec(&TicketPreimage {
        domain: TICKET_DOMAIN,
        contract,
        subject,
        expiry,
    })
    .unwrap_or_else(|_| env::abort())
}

/// La firma en base58 canonico, exactamente 64 bytes (y que vuelva igual al re-codificar).
pub fn decode_ticket_sig(sig: &str) -> Option<[u8; 64]> {
    // base58 de 64 bytes: hasta 88 caracteres (menos con ceros adelante). El tope acota el gas de un `sig` enorme.
    if sig.is_empty() || sig.len() > 88 {
        return None;
    }
    let mut out = [0u8; 64];
    let n = bs58::decode(sig).onto(&mut out).ok()?;
    if n != 64 || bs58::encode(out).into_string() != sig {
        return None;
    }
    Some(out)
}

/// `true` si `ticket.sig` es la firma ed25519 de [`ticket_message`] con `pubkey`. No mira `expiry` (eso es `E54`).
pub fn verify_ticket(
    pubkey: &[u8; 32],
    contract: &str,
    subject: TicketSubject<'_>,
    ticket: &OperatorTicket,
) -> bool {
    match decode_ticket_sig(&ticket.sig) {
        Some(sig) => env::ed25519_verify(
            &sig,
            ticket_message(contract, subject, ticket.expiry),
            pubkey,
        ),
        None => false,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::msg::hex_encode;
    use ed25519_dalek::{Signer, SigningKey};

    /// Llave de los vectores: semilla `01` x 32 (solo para tests y para el SDK; nunca una llave real).
    pub(crate) const SEED: [u8; 32] = [1; 32];
    pub(crate) const CONTRACT: &str = "v2.arras-dev.testnet";
    pub(crate) const EXPIRY: u64 = 1_793_577_600_000_000_000;
    const V1_ID: &str = "DN5eMA3sNES7xsoy6qvy24cVF6Dmwjxpkvd2b4e8P38M";
    /// Recomputados con OpenSSL 4.1.0 (`pkeyutl -sign -rawin`, Ed25519) sobre los mismos bytes: coinciden.
    const T1_MSG: &str = "1800000061727261733a76323a6f70657261746f725f7469636b65741400000076322e61727261732d6465762e746573746e6574002c000000444e35654d4133734e45533778736f7936717679323463564636446d776a78706b766432623465385033384d0000190d5311e418";
    const T1_SIG: &str =
        "4QDFiAQAarug41kHc1UUanhMK4P9pFZw4g3Xh2J15jpJC36ZQVeqP5YCXYzx9jXLhy7mNBKW2MNvgmd5vQ6wEMZs";
    const T2_MSG: &str = "1800000061727261733a76323a6f70657261746f725f7469636b65741400000076322e61727261732d6465762e746573746e65740102000000656d1200000070617965722d64656d6f2e746573746e6574120000006167656e742d64656d6f2e746573746e65740000190d5311e418";
    const T2_SIG: &str =
        "3LBjHxGuTxfHXng6Ze7Y5wQKunwi1HGwYSFoYWRWstrch5UZUWpMTtyYfYFw4oDeB6GB66fwyEcQGd56SruXUuns";

    pub(crate) fn firmar(msg: &[u8]) -> String {
        bs58::encode(SigningKey::from_bytes(&SEED).sign(msg).to_bytes()).into_string()
    }

    pub(crate) fn pubkey() -> [u8; 32] {
        SigningKey::from_bytes(&SEED).verifying_key().to_bytes()
    }

    fn borsh_str(s: &str) -> Vec<u8> {
        let mut v = (s.len() as u32).to_le_bytes().to_vec();
        v.extend_from_slice(s.as_bytes());
        v
    }

    /// El mensaje armado a mano (sin derive): dominio, contrato, tag del enum, sujeto y expiry.
    #[test]
    fn mensaje_byte_a_byte() {
        let mut esperado = borsh_str(TICKET_DOMAIN);
        esperado.extend(borsh_str(CONTRACT));
        esperado.push(0);
        esperado.extend(borsh_str(V1_ID));
        esperado.extend(EXPIRY.to_le_bytes());
        let m = ticket_message(
            CONTRACT,
            TicketSubject::Payment { payment_id: V1_ID },
            EXPIRY,
        );
        assert_eq!(m, esperado);

        let mut esperado = borsh_str(TICKET_DOMAIN);
        esperado.extend(borsh_str(CONTRACT));
        esperado.push(1);
        esperado.extend(borsh_str("em"));
        esperado.extend(borsh_str("payer-demo.testnet"));
        esperado.extend(borsh_str("agent-demo.testnet"));
        esperado.extend(EXPIRY.to_le_bytes());
        let m = ticket_message(
            CONTRACT,
            TicketSubject::Budget {
                operator_id: "em",
                owner: "payer-demo.testnet",
                agent: "agent-demo.testnet",
            },
            EXPIRY,
        );
        assert_eq!(m, esperado);
    }

    /// T-1 y T-2 de `docs/vectores.json`: mensaje, llave publica y firma exactos.
    #[test]
    fn vectores_t1_t2() {
        assert_eq!(
            hex_encode(&pubkey()),
            "8a88e3dd7409f195fd52db2d3cba5d72ca6709bf1d94121bf3748801b40f6f5c"
        );
        let pago = TicketSubject::Payment { payment_id: V1_ID };
        let m = ticket_message(CONTRACT, pago, EXPIRY);
        assert_eq!(hex_encode(&m), T1_MSG);
        assert_eq!(firmar(&m), T1_SIG);
        let budget = TicketSubject::Budget {
            operator_id: "em",
            owner: "payer-demo.testnet",
            agent: "agent-demo.testnet",
        };
        let m = ticket_message(CONTRACT, budget, EXPIRY);
        assert_eq!(hex_encode(&m), T2_MSG);
        assert_eq!(firmar(&m), T2_SIG);
        for (s, subject) in [(T1_SIG, pago), (T2_SIG, budget)] {
            let t = OperatorTicket {
                expiry: EXPIRY,
                sig: s.into(),
            };
            assert!(verify_ticket(&pubkey(), CONTRACT, subject, &t));
        }
    }

    #[test]
    fn verifica_y_rechaza_cualquier_cambio() {
        let pago = TicketSubject::Payment { payment_id: V1_ID };
        let sig = firmar(&ticket_message(CONTRACT, pago, EXPIRY));
        let t = OperatorTicket {
            expiry: EXPIRY,
            sig: sig.clone(),
        };
        assert!(verify_ticket(&pubkey(), CONTRACT, pago, &t));
        // Otro contrato, otro pago, otro expiry, otra llave, otro sujeto.
        assert!(!verify_ticket(&pubkey(), "v2b.arras-dev.testnet", pago, &t));
        assert!(!verify_ticket(
            &pubkey(),
            CONTRACT,
            TicketSubject::Payment {
                payment_id: "Sc4D4Jn3ErWE68FckdDTLzThk1FTR1ZfLzuBUE4EeRm"
            },
            &t
        ));
        let t2 = OperatorTicket {
            expiry: EXPIRY + 1,
            sig: sig.clone(),
        };
        assert!(!verify_ticket(&pubkey(), CONTRACT, pago, &t2));
        let mut otra = pubkey();
        otra[0] ^= 1;
        assert!(!verify_ticket(&otra, CONTRACT, pago, &t));
        let budget = TicketSubject::Budget {
            operator_id: "em",
            owner: "payer-demo.testnet",
            agent: V1_ID,
        };
        assert!(!verify_ticket(&pubkey(), CONTRACT, budget, &t));
    }

    #[test]
    fn firma_mal_formada() {
        let pago = TicketSubject::Payment { payment_id: V1_ID };
        let sig = firmar(&ticket_message(CONTRACT, pago, EXPIRY));
        let bytes = decode_ticket_sig(&sig).expect("firma valida");
        assert_eq!(bs58::encode(bytes).into_string(), sig);
        let mut cero = bytes;
        cero[0] = 0;
        let con_cero = bs58::encode(cero).into_string();
        assert_eq!(
            decode_ticket_sig(&con_cero),
            Some(cero),
            "ceros adelante: mas corta, igual vale"
        );
        // Mal formadas: no decodifican a 64 bytes canonicos.
        for malo in [
            String::new(),
            "0".into(),
            format!("{sig}1"),
            format!("1{sig}"),
            format!("l{}", &sig[1..]),
            "1".repeat(89),
            bs58::encode([7u8; 63]).into_string(),
            bs58::encode([7u8; 65]).into_string(),
            bs58::encode([0u8; 64]).into_string()[1..].to_string(),
        ] {
            assert_eq!(decode_ticket_sig(&malo), None, "{malo}");
            let t = OperatorTicket {
                expiry: EXPIRY,
                sig: malo,
            };
            assert!(!verify_ticket(&pubkey(), CONTRACT, pago, &t));
        }
        // Bien formadas pero de otra cosa: decodifican y la verificacion las rechaza.
        let truncada = sig[..sig.len() - 1].to_string();
        for otra in [truncada, con_cero, bs58::encode([7u8; 64]).into_string()] {
            let t = OperatorTicket {
                expiry: EXPIRY,
                sig: otra,
            };
            assert!(!verify_ticket(&pubkey(), CONTRACT, pago, &t));
        }
    }
}
