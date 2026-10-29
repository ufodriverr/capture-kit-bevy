//! Creature stats, elements and damage. The numbers live in content/rules.ron.

use serde::{Deserialize, Serialize};

use crate::data::Rules;

/// Species base stats (creatures.ron).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SpeciesStats {
    pub hp: f32,
    pub melee_attack: f32,
    pub shot_attack: f32,
    pub defense: f32,
}

/// An individual's talents, 0..=100 each: a talent of 100 adds `talent_bonus` to that stat.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Talents {
    pub hp: u8,
    pub shot: u8,
    pub defense: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CreatureStats {
    pub max_hp: f32,
    pub shot_attack: f32,
    pub melee_attack: f32,
    pub defense: f32,
}

/// stat = species stat x (growth_base + growth_per_level x level) x talent + flat.
pub fn creature_stats(r: &Rules, s: &SpeciesStats, t: &Talents, level: u32) -> CreatureStats {
    let lv = level as f32;
    let tal = |v: u8| 1.0 + r.talent_bonus * f32::from(v) / 100.0;
    let atk = |base: f32, talent: f32| (base * (r.attack_growth_base + r.attack_growth_per_level * lv) * talent + r.attack_flat).floor();
    CreatureStats {
        max_hp: (s.hp * (r.hp_growth_base + r.hp_growth_per_level * lv) * tal(t.hp) + r.hp_flat).floor(),
        shot_attack: atk(s.shot_attack, tal(t.shot)),
        melee_attack: atk(s.melee_attack, 1.0),
        defense: (s.defense * (r.defense_growth_base + r.defense_growth_per_level * lv) * tal(t.defense) + r.defense_flat).floor(),
    }
}

/// Element of a skill, arrow or creature. `None` = no element (plain weapons).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Element {
    None,
    Neutral,
    Fire,
    Water,
    Leaf,
    Spark,
    Frost,
    Earth,
}

impl Element {
    pub fn name(self) -> &'static str {
        match self {
            Element::None => "",
            Element::Neutral => "Neutral",
            Element::Fire => "Fire",
            Element::Water => "Water",
            Element::Leaf => "Leaf",
            Element::Spark => "Spark",
            Element::Frost => "Frost",
            Element::Earth => "Earth",
        }
    }
}

/// x `element_strong` per defending element the attack is strong against (rules.ron `element_chart`),
/// x `element_weak` per defending element that is strong against the attack.
pub fn element_multiplier(r: &Rules, attack: Element, defender: &[Element]) -> f32 {
    let mut m = 1.0;
    for d in defender {
        if r.element_chart.contains(&(attack, *d)) {
            m *= r.element_strong;
        }
        if r.element_chart.contains(&(*d, attack)) {
            m *= r.element_weak;
        }
    }
    m
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    /// Skill power, or the weapon's attack value.
    pub power: f32,
    pub attack: f32,
    pub defense: f32,
    /// The attacker shares the skill's element.
    pub element_match: bool,
    pub element_mult: f32,
    pub is_skill: bool,
    pub creature_to_player: bool,
    pub weak_point: bool,
    pub target_asleep: bool,
    /// Extra multiplier (a wild creature's damage taken, ...).
    pub extra_rate: f32,
}

/// damage_scale x power x attack / (defense + defense_soak), times the rates that apply; at least 1.
pub fn damage(r: &Rules, h: &Hit) -> f32 {
    let mut d = r.damage_scale * h.power * h.attack / (h.defense.max(0.0) + r.defense_soak);
    if h.element_match {
        d *= r.same_element_rate;
    }
    if h.is_skill {
        d *= r.skill_damage_rate;
    }
    if h.creature_to_player {
        d *= r.creature_to_player_rate;
    }
    if h.weak_point {
        d *= r.weak_point_rate;
    }
    if h.target_asleep {
        d *= r.sleeping_rate;
    }
    (d * h.element_mult * h.extra_rate).floor().max(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> Rules {
        crate::data::GameData::load(&crate::data::GameData::default_dir()).unwrap().rules
    }

    #[test]
    fn element_chart() {
        let r = rules();
        assert_eq!(element_multiplier(&r, Element::Fire, &[Element::Leaf]), r.element_strong);
        assert_eq!(element_multiplier(&r, Element::Leaf, &[Element::Fire]), r.element_weak);
        assert_eq!(element_multiplier(&r, Element::Neutral, &[Element::Neutral]), 1.0);
        assert_eq!(element_multiplier(&r, Element::None, &[Element::Fire]), 1.0);
    }

    #[test]
    fn stats_grow_with_level_and_talent() {
        let r = rules();
        let s = SpeciesStats { hp: 75.0, melee_attack: 70.0, shot_attack: 70.0, defense: 70.0 };
        let a = creature_stats(&r, &s, &Talents::default(), 1);
        let b = creature_stats(&r, &s, &Talents { hp: 100, shot: 100, defense: 100 }, 10);
        assert!(b.max_hp > a.max_hp && b.shot_attack > a.shot_attack && b.defense > a.defense);
    }

    #[test]
    fn damage_is_at_least_one_and_scales() {
        let r = rules();
        let h = Hit { power: 60.0, attack: 100.0, defense: 90.0, element_match: false, element_mult: 1.0, is_skill: false, creature_to_player: false, weak_point: false, target_asleep: false, extra_rate: 1.0 };
        let base = damage(&r, &h);
        assert!(base > 1.0);
        assert!(damage(&r, &Hit { element_mult: 1.5, ..h }) > base);
        assert_eq!(damage(&r, &Hit { power: 0.0, ..h }), 1.0);
    }
}
