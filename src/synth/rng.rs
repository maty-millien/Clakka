pub const GOLDEN: u64 = 0x9E37_79B9_7F4A_7C15;

pub fn splitmix64(z: u64) -> u64 {
    let mut z = z.wrapping_add(GOLDEN);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[derive(Clone, Copy)]
pub struct Xorshift32(u32);

impl Xorshift32 {
    pub const ZERO: Xorshift32 = Xorshift32(0x6D2B_79F5);

    #[inline(always)]
    pub fn new(seed: u32) -> Self {
        let s = splitmix64(u64::from(seed)) as u32;
        Xorshift32(if s == 0 { 0x6D2B_79F5 } else { s })
    }

    #[inline(always)]
    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }

    #[inline(always)]
    pub fn uniform(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 * (1.0 / 16_777_216.0)
    }

    #[inline(always)]
    pub fn bipolar(&mut self) -> f32 {
        (self.next_u32() as i32) as f32 * (1.0 / 2_147_483_648.0)
    }

    #[inline(always)]
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.uniform()
    }

    #[inline(always)]
    pub fn gauss(&mut self) -> f32 {
        let s = self.uniform() + self.uniform() + self.uniform() + self.uniform();
        (s - 2.0) * 1.732_050_8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitmix_reference() {
        assert_eq!(splitmix64(0), 0xE220_A839_7B1D_CDAF);
    }

    #[test]
    fn gauss_moments() {
        let mut r = Xorshift32::new(7);
        let n = 200_000;
        let (mut m, mut v) = (0.0f64, 0.0f64);
        for _ in 0..n {
            let g = f64::from(r.gauss());
            m += g;
            v += g * g;
        }
        m /= f64::from(n);
        v = v / f64::from(n) - m * m;
        assert!(m.abs() < 0.01 && (v - 1.0).abs() < 0.02, "{m} {v}");
    }
}
