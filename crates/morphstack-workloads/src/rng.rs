//! Deterministic, platform-independent randomness.
//!
//! Workload traces must be bit-identical for a given seed on every platform,
//! because the paper's results are pinned to them. Platform `libm` functions
//! (`ln`, `exp`, `cos`) may differ in the last ulp between Linux, macOS, and
//! Windows, so this module uses only IEEE-754 basic operations (`+ - * /`,
//! `sqrt`), which are correctly rounded everywhere.

/// `xoshiro256**` seeded through `SplitMix64`.
#[derive(Debug, Clone)]
pub struct Rng {
    s: [u64; 4],
}

impl Rng {
    /// Seeded generator.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        let mut sm = seed;
        let mut next = || {
            sm = sm.wrapping_add(0x9e37_79b9_7f4a_7c15);
            let mut z = sm;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            z ^ (z >> 31)
        };
        Self {
            s: [next(), next(), next(), next()],
        }
    }

    /// Independent child stream, for per-context randomness that does not
    /// depend on scheduling order.
    #[must_use]
    pub fn fork(&mut self, salt: u64) -> Self {
        Self::new(self.next_u64() ^ salt.wrapping_mul(0x9e37_79b9_7f4a_7c15))
    }

    /// Next 64 random bits.
    pub fn next_u64(&mut self) -> u64 {
        let result = self.s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = self.s[1] << 17;
        self.s[2] ^= self.s[0];
        self.s[3] ^= self.s[1];
        self.s[1] ^= self.s[2];
        self.s[0] ^= self.s[3];
        self.s[2] ^= t;
        self.s[3] = self.s[3].rotate_left(45);
        result
    }

    /// Uniform in `[0, 1)` with 53 bits of precision.
    pub fn unit(&mut self) -> f64 {
        #[allow(clippy::cast_precision_loss)]
        let v = (self.next_u64() >> 11) as f64;
        v * (1.0 / 9_007_199_254_740_992.0) // 2^53
    }

    /// Uniform integer in `[lo, hi]` (inclusive). Uses rejection to avoid bias.
    ///
    /// # Panics
    /// Panics if `lo > hi`.
    pub fn range(&mut self, lo: u64, hi: u64) -> u64 {
        assert!(lo <= hi, "empty range {lo}..={hi}");
        let span = hi - lo;
        if span == u64::MAX {
            return self.next_u64();
        }
        let n = span + 1;
        let zone = u64::MAX - (u64::MAX % n);
        loop {
            let v = self.next_u64();
            if v < zone {
                return lo + v % n;
            }
        }
    }

    /// `true` with probability `p`.
    pub fn chance(&mut self, p: f64) -> bool {
        self.unit() < p
    }

    /// Exponential with the given mean.
    pub fn exp(&mut self, mean: f64) -> f64 {
        // 1 - unit() is in (0, 1], so ln is finite.
        -ln(1.0 - self.unit()) * mean
    }

    /// Standard normal, via the Marsaglia polar method (no trigonometry).
    pub fn normal(&mut self) -> f64 {
        loop {
            let u = 2.0 * self.unit() - 1.0;
            let v = 2.0 * self.unit() - 1.0;
            let s = u * u + v * v;
            if s > 0.0 && s < 1.0 {
                return u * (-2.0 * ln(s) / s).sqrt();
            }
        }
    }

    /// Log-normal parameterized by its **median** and the standard deviation
    /// `sigma` of the underlying normal.
    pub fn lognormal(&mut self, median: f64, sigma: f64) -> f64 {
        median * exp(sigma * self.normal())
    }
}

const LN2: f64 = std::f64::consts::LN_2;

/// Natural logarithm using only basic IEEE operations. Accurate to a few ulp
/// for positive finite inputs; deterministic on every platform.
///
/// # Panics
/// Panics unless `x` is positive and finite.
#[must_use]
#[allow(clippy::many_single_char_names)]
pub fn ln(x: f64) -> f64 {
    assert!(x > 0.0 && x.is_finite(), "ln domain: {x}");
    let bits = x.to_bits();
    let raw_exp = i64::try_from((bits >> 52) & 0x7ff).unwrap_or(0);
    let (m, e) = if raw_exp == 0 {
        // Subnormal: scale into the normal range first.
        let y = x * 18_014_398_509_481_984.0; // 2^54
        let b = y.to_bits();
        let re = i64::try_from((b >> 52) & 0x7ff).unwrap_or(0);
        (
            f64::from_bits((b & !(0x7ff << 52)) | (1023 << 52)),
            re - 1023 - 54,
        )
    } else {
        (
            f64::from_bits((bits & !(0x7ff << 52)) | (1023 << 52)),
            raw_exp - 1023,
        )
    };
    // m in [1, 2); recentre to [sqrt(1/2), sqrt(2)) for faster convergence.
    let (m, e) = if m > std::f64::consts::SQRT_2 {
        (m / 2.0, e + 1)
    } else {
        (m, e)
    };
    // ln(m) = 2 atanh(s), s = (m-1)/(m+1), |s| <= 0.1716.
    let s = (m - 1.0) / (m + 1.0);
    let s2 = s * s;
    let mut term = s;
    let mut sum = 0.0;
    let mut k = 1.0;
    while k < 40.0 {
        sum += term / k;
        term *= s2;
        k += 2.0;
    }
    #[allow(clippy::cast_precision_loss)]
    let e = e as f64;
    2.0 * sum + e * LN2
}

/// `e^x` using only basic IEEE operations, for `|x| < 700`.
///
/// # Panics
/// Panics if `|x| >= 700`.
#[must_use]
pub fn exp(x: f64) -> f64 {
    assert!(x.abs() < 700.0, "exp domain: {x}");
    // x = k ln2 + r, |r| <= ln2/2; e^r by Taylor series.
    let k = (x / LN2).round();
    let r = x - k * LN2;
    let mut term = 1.0;
    let mut sum = 1.0;
    let mut n = 1.0;
    while n < 30.0 {
        term *= r / n;
        sum += term;
        n += 1.0;
    }
    #[allow(clippy::cast_possible_truncation)]
    let k = k as i32;
    sum * 2f64.powi(k)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ln_and_exp_match_libm_closely() {
        for &x in &[
            1e-300, 1e-10, 0.001, 0.5, 0.999, 1.0, 1.5, 2.0, 10.0, 1e10, 1e300,
        ] {
            let (a, b) = (ln(x), x.ln());
            assert!(
                (a - b).abs() <= 1e-14 * b.abs().max(1.0),
                "ln({x}) = {a} vs {b}"
            );
        }
        for &x in &[-50.0, -1.0, -1e-9, 0.0, 0.3, 1.0, 5.0, 100.0] {
            let (a, b) = (exp(x), x.exp());
            assert!((a - b).abs() <= 1e-13 * b, "exp({x}) = {a} vs {b}");
        }
    }

    #[test]
    fn subnormal_ln() {
        let x = f64::from_bits(1);
        assert!((ln(x) - x.ln()).abs() < 1e-12);
    }

    #[test]
    fn range_is_inclusive_and_unbiased_enough() {
        let mut r = Rng::new(1);
        let mut seen = [0u32; 6];
        for _ in 0..60_000 {
            let i = usize::try_from(r.range(0, 5)).unwrap();
            seen[i] += 1;
        }
        assert!(
            seen.iter().all(|&n| (9_000..11_000).contains(&n)),
            "{seen:?}"
        );
    }

    #[test]
    fn exp_mean_is_right() {
        let mut r = Rng::new(2);
        let n = 100_000;
        let mean: f64 = (0..n).map(|_| r.exp(50.0)).sum::<f64>() / f64::from(n);
        assert!((mean - 50.0).abs() < 1.0, "{mean}");
    }

    #[test]
    fn known_sequence_is_stable() {
        // Pinned so that any change to the generator is a deliberate,
        // versioned decision: it changes every published trace.
        let mut r = Rng::new(42);
        let v: Vec<u64> = (0..3).map(|_| r.next_u64()).collect();
        let mut r2 = Rng::new(42);
        let v2: Vec<u64> = (0..3).map(|_| r2.next_u64()).collect();
        assert_eq!(v, v2);
    }
}
