//! El `getSummary` de `ReputationRegistryUpgradeable`, en 256 bits.
//!
//! EVM: cada `value` se normaliza a 18 decimales (`value * 10^(18 - valueDecimals)`, en `int256`), se suman, se divide
//! por la cantidad (division entera de Solidity: trunca hacia cero), y el promedio se escala a la **moda** de los
//! `valueDecimals` (el que mas aparece; en un empate gana el menor) con otra division que trunca hacia cero. Aca se
//! hace lo mismo con signo y magnitud de 256 bits: `|value| <= 2^127`, `10^18 < 2^60` y la cantidad entra en `u64`,
//! asi que la suma entra siempre en 256 bits (`2^64 * 2^127 * 2^60 < 2^256`).
//!
//! Desviacion (R-4 del plan): EVM devuelve `int128(promedio escalado)`, un cast que trunca en silencio si el resultado
//! no entra en `int128` (p. ej. un `1e38` con 0 decimales y la moda en 18). Aca el resultado es el numero exacto, como
//! string decimal.

/// Entero sin signo de 256 bits, cuatro palabras de 64 bits en little-endian. El orden NO se deriva: el derivado
/// compararia primero la palabra menos significativa (refutador, P1); [`Ord`] compara desde la mas significativa.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct U256([u64; 4]);

impl Ord for U256 {
    fn cmp(&self, o: &Self) -> std::cmp::Ordering {
        self.0.iter().rev().cmp(o.0.iter().rev())
    }
}

impl PartialOrd for U256 {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(o))
    }
}

impl U256 {
    pub const ZERO: Self = Self([0; 4]);

    pub fn from_u128(x: u128) -> Self {
        Self([x as u64, (x >> 64) as u64, 0, 0])
    }

    pub fn is_zero(&self) -> bool {
        self.0 == [0; 4]
    }

    /// `a * b`, que siempre entra (192 bits).
    pub fn mul_u128_u64(a: u128, b: u64) -> Self {
        let lo = (a as u64 as u128) * (b as u128);
        let hi = ((a >> 64) as u64 as u128) * (b as u128);
        // a*b = lo + hi * 2^64
        let w0 = lo as u64;
        let carry = (lo >> 64) + (hi as u64 as u128);
        let w1 = carry as u64;
        let w2 = ((carry >> 64) + (hi >> 64)) as u64;
        Self([w0, w1, w2, 0])
    }

    /// `self + o`; `None` si desborda.
    pub fn checked_add(self, o: Self) -> Option<Self> {
        let mut out = [0u64; 4];
        let mut carry = 0u128;
        for (i, w) in out.iter_mut().enumerate() {
            let s = self.0[i] as u128 + o.0[i] as u128 + carry;
            *w = s as u64;
            carry = s >> 64;
        }
        if carry == 0 { Some(Self(out)) } else { None }
    }

    /// `self - o` con `self >= o`.
    fn sub(self, o: Self) -> Self {
        let mut out = [0u64; 4];
        let mut borrow = 0i128;
        for (i, w) in out.iter_mut().enumerate() {
            let d = self.0[i] as i128 - o.0[i] as i128 - borrow;
            if d < 0 {
                *w = (d + (1i128 << 64)) as u64;
                borrow = 1;
            } else {
                *w = d as u64;
                borrow = 0;
            }
        }
        Self(out)
    }

    /// `(self / d, self % d)`, con `d > 0`.
    pub fn div_rem_u64(self, d: u64) -> (Self, u64) {
        let mut out = [0u64; 4];
        let mut rem = 0u128;
        for i in (0..4).rev() {
            let cur = (rem << 64) | self.0[i] as u128;
            out[i] = (cur / d as u128) as u64;
            rem = cur % d as u128;
        }
        (Self(out), rem as u64)
    }

    /// Decimal, sin ceros a la izquierda.
    pub fn to_dec_string(self) -> String {
        if self.is_zero() {
            return "0".into();
        }
        let mut digits = Vec::new();
        let mut n = self;
        while !n.is_zero() {
            let (q, r) = n.div_rem_u64(10);
            digits.push(b'0' + r as u8);
            n = q;
        }
        digits.reverse();
        String::from_utf8(digits).unwrap_or_default()
    }
}

/// `10^n` para `n <= 18`.
pub fn pow10(n: u8) -> u64 {
    10u64.pow(u32::from(n))
}

/// El resultado de `getSummary`: `(count, summaryValue, summaryValueDecimals)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Summary {
    pub count: u64,
    /// Decimal con signo, exacto (`"0"` sin calificaciones).
    pub value: String,
    pub decimals: u8,
}

/// Acumulador del promedio. `add` recibe `(value, value_decimals)` ya validados (`value_decimals <= 18`).
#[derive(Clone, Debug, Default)]
pub struct Accumulator {
    pos: U256,
    neg: U256,
    count: u64,
    decimal_counts: [u64; 19],
}

impl Accumulator {
    pub fn add(&mut self, value: i128, decimals: u8) {
        let d = decimals.min(18);
        let n = U256::mul_u128_u64(value.unsigned_abs(), pow10(18 - d));
        // No desborda (ver el doc del modulo); si desbordara, se abortaria antes que devolver un numero falso.
        let slot = if value < 0 {
            &mut self.neg
        } else {
            &mut self.pos
        };
        *slot = slot
            .checked_add(n)
            .unwrap_or_else(|| near_sdk::env::abort());
        self.count = self.count.saturating_add(1);
        self.decimal_counts[d as usize] = self.decimal_counts[d as usize].saturating_add(1);
    }

    pub fn count(&self) -> u64 {
        self.count
    }

    pub fn finish(&self) -> Summary {
        if self.count == 0 {
            return Summary {
                count: 0,
                value: "0".into(),
                decimals: 0,
            };
        }
        let mut mode = 0u8;
        let mut max = 0u64;
        for (d, &c) in self.decimal_counts.iter().enumerate() {
            if c > max {
                max = c;
                mode = d as u8;
            }
        }
        let (negative, magnitude) = if self.pos >= self.neg {
            (false, self.pos.sub(self.neg))
        } else {
            (true, self.neg.sub(self.pos))
        };
        let (avg, _) = magnitude.div_rem_u64(self.count);
        let (scaled, _) = avg.div_rem_u64(pow10(18 - mode));
        let digits = scaled.to_dec_string();
        let value = if negative && !scaled.is_zero() {
            format!("-{digits}")
        } else {
            digits
        };
        Summary {
            count: self.count,
            value,
            decimals: mode,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(items: &[(i128, u8)]) -> Summary {
        let mut a = Accumulator::default();
        for &(v, d) in items {
            a.add(v, d);
        }
        a.finish()
    }

    /// La misma cuenta en i128 cuando entra (referencia para casos chicos).
    fn reference(items: &[(i128, u8)]) -> (u64, i128, u8) {
        if items.is_empty() {
            return (0, 0, 0);
        }
        let mut counts = [0u64; 19];
        let mut sum: i128 = 0;
        for &(v, d) in items {
            sum += v * 10i128.pow(18 - d as u32);
            counts[d as usize] += 1;
        }
        let mut mode = 0;
        let mut max = 0;
        for (d, &c) in counts.iter().enumerate() {
            if c > max {
                max = c;
                mode = d;
            }
        }
        let avg = sum / items.len() as i128;
        (
            items.len() as u64,
            avg / 10i128.pow(18 - mode as u32),
            mode as u8,
        )
    }

    #[test]
    fn vacio() {
        assert_eq!(
            summary(&[]),
            Summary {
                count: 0,
                value: "0".into(),
                decimals: 0
            }
        );
    }

    #[test]
    fn ejemplos_de_la_eip() {
        // starred 87 y 91 con 0 decimales -> 89
        assert_eq!(summary(&[(87, 0), (91, 0)]).value, "89");
        // uptime 99,77 % y 98,10 % con 2 decimales -> 98,93 (truncado)
        let s = summary(&[(9977, 2), (9810, 2)]);
        assert_eq!((s.value.as_str(), s.decimals), ("9893", 2));
        // tradingYield -3,2 % y 1,0 % (1 decimal) -> -1,1
        let s = summary(&[(-32, 1), (10, 1)]);
        assert_eq!((s.value.as_str(), s.decimals), ("-11", 1));
    }

    #[test]
    fn moda_y_empates() {
        // 2 con 0 decimales, 1 con 2: moda 0.
        let s = summary(&[(1, 0), (2, 0), (150, 2)]);
        assert_eq!(s.decimals, 0);
        // empate 1-1: gana el menor (0).
        let s = summary(&[(1, 2), (1, 0)]);
        assert_eq!(s.decimals, 0);
        // empate entre 3 y 5: gana 3.
        let s = summary(&[(1, 5), (1, 3)]);
        assert_eq!(s.decimals, 3);
    }

    #[test]
    fn trunca_hacia_cero_como_solidity() {
        // (-1 + 0) / 2 = -0,5 -> 0 (no -1)
        assert_eq!(summary(&[(-1, 0), (0, 0)]).value, "0");
        // -5 / 3 = -1,66 -> -1
        assert_eq!(summary(&[(-5, 0), (0, 0), (0, 0)]).value, "-1");
        // 5 / 3 -> 1
        assert_eq!(summary(&[(5, 0), (0, 0), (0, 0)]).value, "1");
        // escalado: 15 con 1 decimal y 1 con 0 decimales, moda empatada -> 0: (1,5 + 1)/2 = 1,25 -> 1
        assert_eq!(summary(&[(15, 1), (1, 0)]).value, "1");
    }

    #[test]
    fn coincide_con_la_referencia_en_casos_chicos() {
        let casos: &[&[(i128, u8)]] = &[
            &[(87, 0)],
            &[(-87, 0), (13, 0)],
            &[(9977, 2), (1, 0), (-3, 1), (12345, 4)],
            &[(-1_000_000, 6), (999_999, 6), (7, 0)],
            &[(1, 18), (-1, 18), (3, 18)],
            &[(i64::MAX as i128, 0), (i64::MIN as i128, 0), (5, 0)],
        ];
        for c in casos {
            let s = summary(c);
            let (n, v, d) = reference(c);
            assert_eq!(
                (s.count, s.value.clone(), s.decimals),
                (n, v.to_string(), d),
                "{c:?}"
            );
        }
    }

    #[test]
    fn signos_mezclados_de_mas_de_una_palabra() {
        // Refutador, P1: con 0 decimales, un value >= 19 normalizado pasa de 2^64.
        assert_eq!(summary(&[(19, 0), (-1, 0)]).value, "9");
        assert_eq!(summary(&[(5, 0), (-19, 0)]).value, "-7");
        assert_eq!(summary(&[(87, 0), (-87, 0), (13, 0)]).value, "4");
        assert_eq!(summary(&[(-100, 0), (100, 0)]).value, "0");
        assert_eq!(summary(&[(100, 0), (-99, 0), (-100, 0)]).value, "-33");
    }

    #[test]
    fn u256_ordena_desde_la_palabra_mas_significativa() {
        let alto = U256([0, 1, 0, 0]);
        let bajo = U256([u64::MAX, 0, 0, 0]);
        assert!(alto > bajo);
        assert!(U256([0, 0, 0, 1]) > U256([u64::MAX, u64::MAX, u64::MAX, 0]));
        assert!(U256([1, 2, 3, 4]) < U256([0, 3, 3, 4]));
        assert_eq!(
            U256([1, 2, 3, 4]).cmp(&U256([1, 2, 3, 4])),
            std::cmp::Ordering::Equal
        );
    }

    #[test]
    fn diferencial_aleatorio_contra_la_referencia() {
        // |value| <= 10^18 y hasta 100 calificaciones: la referencia en i128 no desborda (100 * 10^18 * 10^18 < 2^127).
        let mut semilla: u64 = 0x8004;
        let mut rnd = |n: u64| {
            semilla = semilla
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (semilla >> 33) % n
        };
        for _ in 0..2_000 {
            let n = 1 + rnd(100) as usize;
            let items: Vec<(i128, u8)> = (0..n)
                .map(|_| {
                    let mag = match rnd(3) {
                        0 => rnd(101) as i128,
                        1 => rnd(1_000_000) as i128,
                        _ => (rnd(1_000_000_000) as i128) * (rnd(1_000_000_000) as i128 + 1),
                    }
                    .min(10i128.pow(18));
                    let v = if rnd(2) == 0 { -mag } else { mag };
                    (v, rnd(19) as u8)
                })
                .collect();
            let s = summary(&items);
            let (c, v, d) = reference(&items);
            assert_eq!(
                (s.count, s.value.clone(), s.decimals),
                (c, v.to_string(), d),
                "{items:?}"
            );
        }
    }

    #[test]
    fn exacto_donde_evm_trunca_int128() {
        // 1e38 con 0 decimales y dos con 18: la moda es 18 y el promedio escalado ~3,3e55 no entra en int128.
        let max = MAX_ABS as i128;
        let s = summary(&[(max, 0), (1, 18), (1, 18)]);
        assert_eq!(s.decimals, 18);
        // (1e56 + 2) / 3 = 33333...3334 (56 digitos): 1e56/3 = 3,33e55 y (1e56+2)/3 es exacto.
        assert_eq!(s.value, format!("{}4", "3".repeat(55)));
        let s = summary(&[(-max, 0), (-max, 0)]);
        assert_eq!(s.value, format!("-{}", MAX_ABS));
    }

    const MAX_ABS: u128 = crate::validate::MAX_ABS_VALUE;

    #[test]
    fn u256_basico() {
        let a = U256::mul_u128_u64(u128::MAX, u64::MAX);
        // (2^128-1)(2^64-1) = 2^192 - 2^128 - 2^64 + 1
        assert_eq!(a.0, [1, u64::MAX, u64::MAX - 1, 0]);
        assert_eq!(U256::from_u128(12345).to_dec_string(), "12345");
        assert_eq!(U256::ZERO.to_dec_string(), "0");
        assert_eq!(
            U256::from_u128(u128::MAX).to_dec_string(),
            u128::MAX.to_string()
        );
        let big = U256([0, 0, 0, u64::MAX]);
        assert!(big.checked_add(big).is_none());
        assert_eq!(
            U256::from_u128(10).sub(U256::from_u128(3)),
            U256::from_u128(7)
        );
        assert_eq!(
            U256([0, 1, 0, 0]).sub(U256::from_u128(1)),
            U256::from_u128(u64::MAX as u128)
        );
        let (q, r) = U256::from_u128(1_000_000_007).div_rem_u64(10);
        assert_eq!((q, r), (U256::from_u128(100_000_000), 7));
    }
}
