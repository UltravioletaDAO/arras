//! Tarifas (sec. 3.8): `fee = floor(x * fee_bps / 10000)`, `net = x - fee`, sobre lo liberado en cada llamada,
//! igual que Execution Market (`fees.py:92-103`).

use crate::BPS_DENOMINATOR;

/// Reparto de un monto liberado entre el receiver (`net`) y el `fee_recipient` (`fee`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FeeSplit {
    pub fee: u128,
    pub net: u128,
}

/// `fee = floor(x * fee_bps / 10000)` sin desbordar: con `x = 10000 q + r`,
/// `floor(x * b / 10000) = q * b + floor(r * b / 10000)` (exacto porque `q * b` es entero).
/// Devuelve `None` si `fee_bps > 10000` o si algo desborda (no pasa con montos de un `u128` real).
pub fn fee_split(x: u128, fee_bps: u16) -> Option<FeeSplit> {
    let bps = u128::from(fee_bps);
    if bps > BPS_DENOMINATOR {
        return None;
    }
    let q = x / BPS_DENOMINATOR;
    let r = x % BPS_DENOMINATOR;
    let fee = q
        .checked_mul(bps)?
        .checked_add(r.checked_mul(bps)? / BPS_DENOMINATOR)?;
    let net = x.checked_sub(fee)?;
    Some(FeeSplit { fee, net })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// U-02 `fee_vectores`: F-1..F-6 de la sec. 7 (y F-7, el `min_amount` de 0,01 USDC).
    #[test]
    fn fee_vectores() {
        let casos: [(&str, u128, u16, u128, u128); 7] = [
            ("F-1", 1_000_000, 1300, 130_000, 870_000),
            ("F-2", 1, 1300, 0, 1),
            ("F-3", 10_000_000, 1300, 1_300_000, 8_700_000),
            ("F-4", 400_000, 1300, 52_000, 348_000),
            ("F-5", 600_000, 1300, 78_000, 522_000),
            ("F-6", 10_000_000, 1800, 1_800_000, 8_200_000),
            ("F-7", 10_000, 1300, 1_300, 8_700),
        ];
        for (nombre, x, bps, fee, net) in casos {
            assert_eq!(fee_split(x, bps), Some(FeeSplit { fee, net }), "{nombre}");
        }
    }

    #[test]
    fn fee_igual_a_la_formula_ingenua_y_sin_desborde() {
        for x in [
            0u128,
            1,
            9_999,
            10_000,
            10_001,
            123_456_789,
            u128::from(u64::MAX),
            u128::MAX,
        ] {
            for bps in [0u16, 1, 1300, 1800, 9_999, 10_000] {
                let s = fee_split(x, bps).expect("no desborda");
                assert_eq!(s.fee + s.net, x);
                if let Some(prod) = x.checked_mul(u128::from(bps)) {
                    assert_eq!(s.fee, prod / 10_000, "x={x} bps={bps}");
                }
            }
        }
        assert_eq!(fee_split(1, 10_001), None);
    }
}
