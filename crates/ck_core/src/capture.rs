//! Capture chance (content/rules.ron explains the knobs):
//!   hp_factor    = 1 - capture_hp_weight x (hp / max hp)
//!   level_factor = clamp((orb level - creature level + window) / (2 x window), 0, 1)
//!   chance       = hp_factor x level_factor x species catch_rate x (1 + status bonus) [x back bonus], 0..1
//! The orb then shakes: each shake passes with chance^weight, so all of them pass with exactly `chance`.

use crate::data::Rules;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CaptureInputs {
    pub orb_level: f64,
    pub creature_level: u32,
    pub hp: f64,
    pub max_hp: f64,
    /// Species `catch_rate`.
    pub catch_rate: f64,
    /// Sum of the running statuses' `capture_bonus` (0 = none).
    pub status_bonus: f64,
    /// The creature hasn't noticed the player (a throw from behind / while it sleeps).
    pub unaware: bool,
    pub uncapturable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CaptureBreakdown {
    pub hp_factor: f64,
    pub level_factor: f64,
    pub result: f64,
}

pub fn capture_chance(r: &Rules, i: &CaptureInputs) -> CaptureBreakdown {
    if i.uncapturable {
        return CaptureBreakdown { hp_factor: 0.0, level_factor: 0.0, result: 0.0 };
    }
    let frac = if i.max_hp > 0.0 { (i.hp / i.max_hp).clamp(0.0, 1.0) } else { 0.0 };
    let hp_factor = 1.0 - r.capture_hp_weight * frac;
    let w = r.capture_level_window.max(1.0);
    let level_factor = ((i.orb_level - f64::from(i.creature_level) + w) / (2.0 * w)).clamp(0.0, 1.0);
    let back = if i.unaware { r.capture_back_bonus } else { 1.0 };
    let result = (hp_factor * level_factor * i.catch_rate * (1.0 + i.status_bonus) * back).clamp(0.0, 1.0);
    CaptureBreakdown { hp_factor, level_factor, result }
}

/// How many shakes pass (all of them = captured): shake k passes when `rolls[k] < p^weights[k]`.
pub fn judge_shakes(p: f64, weights: &[f64], rolls: &[f64]) -> usize {
    let p = p.clamp(0.0, 1.0);
    weights.iter().zip(rolls).take_while(|(w, r)| **r < p.powf(**w)).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> Rules {
        crate::data::GameData::load(&crate::data::GameData::default_dir()).unwrap().rules
    }

    fn input(level: u32, hp: f64) -> CaptureInputs {
        CaptureInputs { orb_level: 8.0, creature_level: level, hp, max_hp: 100.0, catch_rate: 1.0, status_bonus: 0.0, unaware: false, uncapturable: false }
    }

    #[test]
    fn lower_hp_and_level_raise_the_chance() {
        let r = rules();
        let full = capture_chance(&r, &input(3, 100.0)).result;
        let low = capture_chance(&r, &input(3, 10.0)).result;
        let high_level = capture_chance(&r, &input(15, 10.0)).result;
        assert!(low > full && low > high_level, "{full} {low} {high_level}");
    }

    #[test]
    fn bonuses_and_flags() {
        let r = rules();
        let base = capture_chance(&r, &input(3, 50.0)).result;
        assert!(capture_chance(&r, &CaptureInputs { status_bonus: 0.25, ..input(3, 50.0) }).result > base);
        assert!(capture_chance(&r, &CaptureInputs { unaware: true, ..input(3, 50.0) }).result > base);
        assert_eq!(capture_chance(&r, &CaptureInputs { uncapturable: true, ..input(1, 1.0) }).result, 0.0);
        assert!(capture_chance(&r, &CaptureInputs { catch_rate: 10.0, ..input(1, 1.0) }).result <= 1.0);
    }

    #[test]
    fn shakes_succeed_with_probability_p() {
        let r = rules();
        let mut rng = crate::rng::GameRng::new(777);
        let n = 20000;
        let ok = (0..n)
            .filter(|_| {
                let rolls: Vec<f64> = (0..r.shake_weights.len()).map(|_| rng.f64()).collect();
                judge_shakes(0.3, &r.shake_weights, &rolls) == r.shake_weights.len()
            })
            .count();
        let rate = ok as f64 / n as f64;
        assert!((rate - 0.3).abs() < 0.015, "{rate}");
    }
}
