//! Seeded random numbers, so scripted checks replay exactly.

use rand_chacha::ChaCha8Rng;
use rand_core::{Rng as _, SeedableRng};

#[derive(Debug, Clone)]
pub struct GameRng(ChaCha8Rng);

impl GameRng {
    pub fn new(seed: u64) -> Self {
        Self(ChaCha8Rng::seed_from_u64(seed))
    }

    /// Uniform in [0, 1).
    pub fn f64(&mut self) -> f64 {
        (self.0.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    pub fn f32(&mut self) -> f32 {
        self.f64() as f32
    }

    /// Uniform in [lo, hi].
    pub fn range_i32(&mut self, lo: i32, hi: i32) -> i32 {
        if hi <= lo {
            return lo;
        }
        lo + (self.0.next_u64() % (hi - lo + 1) as u64) as i32
    }

    pub fn range_f32(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.f32()
    }

    /// True with probability `p` (0..1).
    pub fn chance(&mut self, p: f64) -> bool {
        self.f64() < p
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn deterministic_and_in_range() {
        let (mut a, mut b) = (super::GameRng::new(7), super::GameRng::new(7));
        for _ in 0..1000 {
            let x = a.f64();
            assert_eq!(x, b.f64());
            assert!((0.0..1.0).contains(&x));
            let r = a.range_i32(1, 3);
            b.range_i32(1, 3);
            assert!((1..=3).contains(&r));
        }
    }
}
