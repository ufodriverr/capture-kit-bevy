//! Creature individuals and where they live. An individual (stable id, level, talents, skills, needs) is
//! separate from its spawned actor: the actor can be recalled, stored or reassigned while the individual
//! stays the same.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::data::{Creature, GameData, Rules};
use crate::rng::GameRng;
use crate::stats::{CreatureStats, SpeciesStats, Talents, creature_stats};

pub type CreatureId = u64;

/// Party slots.
pub const PARTY_SIZE: usize = 5;
/// Creatures kept in the Camp Core's storage (plus the ones working at the camp).
pub const STORAGE_SIZE: usize = 120;
/// Active skill slots.
pub const EQUIP_SLOTS: usize = 3;
/// Work priority every job starts at: the least important, so raising one job is one click.
pub const DEFAULT_WORK_PRIO: u8 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Gender {
    Male,
    Female,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Place {
    Wild,
    Party(usize),
    /// In the Camp Core's storage.
    Storage,
    /// Living and working at the camp (still owned).
    Camp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Individual {
    pub id: CreatureId,
    pub species: String,
    pub level: u32,
    /// Experience inside the current level.
    pub exp: u32,
    pub gender: Gender,
    pub talents: Talents,
    pub hp: f32,
    pub learned: Vec<String>,
    pub equipped: Vec<String>,
    /// Stomach (species `stomach` when full).
    pub fullness: f32,
    /// Mood, 0..100.
    pub mood: f32,
    pub place: Place,
    pub owned: bool,
    /// Knocked out (0 HP): can't be summoned until it recovers.
    pub incapacitated: bool,
    pub nickname: Option<String>,
    /// Job priorities: job id -> 1 (first) .. 5; 0 = never. Missing = `DEFAULT_WORK_PRIO`.
    #[serde(default)]
    pub work_prio: BTreeMap<String, u8>,
}

impl Individual {
    /// The priority number for a job (1 = first, 0 = never).
    pub fn work_priority(&self, job: &str) -> u8 {
        self.work_prio.get(job).copied().unwrap_or(DEFAULT_WORK_PRIO)
    }

    pub fn stats(&self, r: &Rules, sp: &Creature) -> CreatureStats {
        let s = SpeciesStats { hp: sp.hp, melee_attack: sp.melee, shot_attack: sp.shot, defense: sp.defense };
        creature_stats(r, &s, &self.talents, self.level)
    }

    /// Equips a learned skill, or unequips it if equipped (`EQUIP_SLOTS` at most).
    pub fn toggle_equip(&mut self, skill: &str) -> Result<bool, String> {
        if let Some(i) = self.equipped.iter().position(|w| w == skill) {
            self.equipped.remove(i);
            return Ok(false);
        }
        if !self.learned.iter().any(|w| w == skill) {
            return Err(format!("{skill} is not learned"));
        }
        if self.equipped.len() >= EQUIP_SLOTS {
            return Err(format!("all {EQUIP_SLOTS} skill slots are used; unequip one first"));
        }
        self.equipped.push(skill.to_owned());
        Ok(true)
    }

    pub fn name<'a>(&'a self, sp: &'a Creature) -> &'a str {
        self.nickname.as_deref().unwrap_or(&sp.name)
    }
}

/// Every creature individual: wild ones (while their actor lives), the party, storage and the camp.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Roster {
    pub all: BTreeMap<CreatureId, Individual>,
    pub next_id: CreatureId,
}

impl Roster {
    /// A new individual of `species` at `level` with random talents and gender; skills learned up to its
    /// level, the newest `EQUIP_SLOTS` equipped.
    pub fn create(&mut self, d: &GameData, species: &str, level: u32, rng: &mut GameRng) -> CreatureId {
        self.next_id += 1;
        let id = self.next_id;
        let sp = &d.creatures[species];
        let male = rng.f32() < sp.male_chance;
        let talents = Talents { hp: rng.range_i32(0, 100) as u8, shot: rng.range_i32(0, 100) as u8, defense: rng.range_i32(0, 100) as u8 };
        let learned: Vec<String> = sp.learnset.iter().filter(|(lv, _)| *lv <= level).map(|(_, w)| w.clone()).collect();
        let equipped = learned.iter().rev().take(EQUIP_SLOTS).cloned().collect();
        let mut ind = Individual {
            id,
            species: species.to_owned(),
            level,
            exp: 0,
            gender: if male { Gender::Male } else { Gender::Female },
            talents,
            hp: 0.0,
            learned,
            equipped,
            fullness: sp.stomach,
            mood: 100.0,
            place: Place::Wild,
            owned: false,
            incapacitated: false,
            nickname: None,
            work_prio: Default::default(),
        };
        ind.hp = ind.stats(&d.rules, sp).max_hp;
        self.all.insert(id, ind);
        id
    }

    pub fn get(&self, id: CreatureId) -> Option<&Individual> {
        self.all.get(&id)
    }

    pub fn get_mut(&mut self, id: CreatureId) -> Option<&mut Individual> {
        self.all.get_mut(&id)
    }

    pub fn party(&self) -> [Option<CreatureId>; PARTY_SIZE] {
        let mut out = [None; PARTY_SIZE];
        for p in self.all.values() {
            if let Place::Party(i) = p.place
                && i < PARTY_SIZE
            {
                out[i] = Some(p.id);
            }
        }
        out
    }

    pub fn in_place(&self, place: Place) -> Vec<CreatureId> {
        self.all.values().filter(|p| p.place == place).map(|p| p.id).collect()
    }

    pub fn stored_len(&self) -> usize {
        self.all.values().filter(|p| p.owned && matches!(p.place, Place::Storage | Place::Camp)).count()
    }

    /// Captured: the individual becomes owned and goes to the first free party slot, else to storage.
    /// The wild individual and the owned one are the same id.
    pub fn take_ownership(&mut self, id: CreatureId) -> Result<Place, String> {
        let party = self.party();
        let full = self.stored_len() >= STORAGE_SIZE;
        let p = self.all.get_mut(&id).ok_or("no such creature")?;
        if p.owned {
            return Err("already owned".into());
        }
        let place = match party.iter().position(Option::is_none) {
            Some(i) => Place::Party(i),
            None if !full => Place::Storage,
            None => return Err("party and storage are full".into()),
        };
        p.owned = true;
        p.place = place;
        Ok(place)
    }

    /// Moves an owned creature between the party, storage and the camp, keeping its identity.
    pub fn move_to(&mut self, id: CreatureId, place: Place) -> Result<(), String> {
        if let Place::Party(i) = place {
            if i >= PARTY_SIZE {
                return Err("no such party slot".into());
            }
            if let Some(other) = self.party()[i]
                && other != id
            {
                // Swap: the occupant takes this creature's old place.
                let old = self.all[&id].place;
                self.all.get_mut(&other).ok_or("missing")?.place = if matches!(old, Place::Party(_)) { old } else { Place::Storage };
            }
        } else if place == Place::Storage && self.stored_len() >= STORAGE_SIZE && !matches!(self.all.get(&id).map(|p| p.place), Some(Place::Storage | Place::Camp)) {
            return Err("storage is full".into());
        }
        let p = self.all.get_mut(&id).ok_or("no such creature")?;
        if !p.owned {
            return Err("not owned".into());
        }
        p.place = place;
        Ok(())
    }

    /// Removes a wild individual (defeated or despawned).
    pub fn remove_wild(&mut self, id: CreatureId) {
        if self.all.get(&id).is_some_and(|p| !p.owned) {
            self.all.remove(&id);
        }
    }
}

/// Experience to go from `level` to `level + 1`: exp_base x level ^ exp_power (the same for the player and
/// creatures). None at the max level.
pub fn next_exp(r: &Rules, level: u32) -> Option<u32> {
    (level < r.max_level).then(|| (r.exp_base * (level as f32).powf(r.exp_power)).round().max(1.0) as u32)
}

/// Adds experience; returns the levels gained.
pub fn add_exp(r: &Rules, level: &mut u32, exp: &mut u32, amount: u32) -> u32 {
    *exp += amount;
    let mut gained = 0;
    while let Some(need) = next_exp(r, *level) {
        if *exp < need {
            break;
        }
        *exp -= need;
        *level += 1;
        gained += 1;
    }
    gained
}

/// Experience for defeating (or capturing) a creature of `level`.
pub fn defeat_exp(r: &Rules, sp: &Creature, level: u32) -> u32 {
    ((r.defeat_exp_base + r.defeat_exp_per_level * level as f32) * sp.exp_rate.max(0.0)).round() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d() -> GameData {
        GameData::load(&GameData::default_dir()).unwrap()
    }

    #[test]
    fn equip_rules() {
        let d = d();
        let mut w = Roster::default();
        let id = w.create(&d, "mossnut", 30, &mut GameRng::new(4));
        let p = w.get_mut(id).unwrap();
        assert_eq!(p.learned.len(), 3);
        assert_eq!(p.equipped.len(), EQUIP_SLOTS);
        let first = p.equipped[0].clone();
        assert_eq!(p.toggle_equip(&first), Ok(false));
        assert_eq!(p.toggle_equip(&first), Ok(true));
        assert!(p.toggle_equip("not_a_skill").is_err());
    }

    #[test]
    fn skills_by_level() {
        let d = d();
        let mut w = Roster::default();
        let mut rng = GameRng::new(3);
        let id = w.create(&d, "fluffin", 1, &mut rng);
        assert_eq!(w.get(id).unwrap().learned, vec!["hop_kick".to_owned()]);
        let id5 = w.create(&d, "fluffin", 5, &mut rng);
        assert_eq!(w.get(id5).unwrap().learned.len(), 2);
    }

    #[test]
    fn capture_fills_party_then_storage() {
        let d = d();
        let mut w = Roster::default();
        let mut rng = GameRng::new(3);
        let ids: Vec<_> = (0..7).map(|_| w.create(&d, "fluffin", 2, &mut rng)).collect();
        for (i, id) in ids.iter().enumerate() {
            let place = w.take_ownership(*id).unwrap();
            assert_eq!(place, if i < 5 { Place::Party(i) } else { Place::Storage });
        }
        assert!(w.take_ownership(ids[0]).is_err(), "can't capture twice");
        w.move_to(ids[6], Place::Party(0)).unwrap();
        assert_eq!(w.party()[0], Some(ids[6]));
        assert_eq!(w.get(ids[0]).unwrap().place, Place::Storage);
    }

    #[test]
    fn levels_from_the_curve() {
        let d = d();
        let need = next_exp(&d.rules, 1).unwrap();
        let (mut lv, mut xp) = (1, 0);
        assert_eq!(add_exp(&d.rules, &mut lv, &mut xp, need - 1), 0);
        assert_eq!(add_exp(&d.rules, &mut lv, &mut xp, 1), 1);
        assert_eq!((lv, xp), (2, 0));
        assert!(next_exp(&d.rules, 10).unwrap() > need);
    }
}
