//! The player's items: bag, 4 weapon slots (mouse wheel), head and body armour, glider, the selected orb
//! type (2 cycles it) and arrow type (hold R). Weight limit: rules.ron `carry_weight` + `point_weight` per
//! status point.

use bevy::prelude::*;
use ck_core::inventory::{Container, ItemInfo, Stack};

use crate::data::Db;
use crate::input::PlayerInput;
use crate::log::{GameLog, Say};
use crate::player::{Locomotion, Player};

/// Main bag slot count.
pub const BAG_SLOTS: usize = 42;
pub const WEAPON_SLOTS: usize = 4;
/// Bow ammo item ids.
pub const ARROWS: [&str; 3] = ["arrow", "fire_arrow", "venom_arrow"];

#[derive(Component, Debug, Clone)]
pub struct Inventory {
    pub bag: Container,
    pub weapons: Vec<Option<Stack>>,
    pub selected: usize,
    pub armor: Option<Stack>,
    /// Head armour (e.g. the Feather Band).
    pub head: Option<Stack>,
    pub glider: Option<Stack>,
    pub orb: String,
    /// Arrows loaded in the bow, all of type `ammo`.
    pub loaded: u32,
    /// The arrow type the bow loads (one of `ARROWS`).
    pub ammo: String,
    pub weight_points: u32,
    /// Extra carry weight from `Carry` bond skills in the party (set every frame).
    pub bond_weight: f32,
}

impl Inventory {
    pub fn new() -> Self {
        Self { bag: Container::new(BAG_SLOTS), weapons: vec![None; WEAPON_SLOTS], selected: 0, armor: None, head: None, glider: None, orb: "capture_orb".into(), loaded: 0, ammo: "arrow".into(), weight_points: 0, bond_weight: 0.0 }
    }

    pub fn weapon(&self) -> Option<&Stack> {
        self.weapons.get(self.selected).and_then(Option::as_ref)
    }

    pub fn weight(&self, db: &Db) -> f32 {
        let info: &dyn ItemInfo = &*db.data;
        let extra: f32 = self.weapons.iter().chain([&self.armor, &self.head, &self.glider]).flatten().map(|s| info.weight(&s.item) * s.count as f32).sum();
        self.bag.weight(info) + extra
    }

    pub fn max_weight(&self, db: &Db) -> f32 {
        db.rules().carry_weight + self.weight_points as f32 * db.rules().point_weight + self.bond_weight
    }

    /// Adds to the bag; returns what didn't fit.
    pub fn give(&mut self, db: &Db, item: &str, count: u32) -> u32 {
        let id = db.item_id(item).unwrap_or(item).to_owned();
        self.bag.add(&id, count, &*db.data)
    }

    pub fn count(&self, item: &str) -> u32 {
        self.bag.count(item)
    }

    /// Moves a bag slot into its equipment place (weapon -> first free weapon slot, armour, glider).
    pub fn equip_from_bag(&mut self, db: &Db, slot: usize) -> Result<String, String> {
        let Some(stack) = self.bag.slots.get(slot).cloned().flatten() else { return Err("empty slot".into()) };
        let item = db.item(&stack.item).ok_or("unknown item")?;
        let name = item.name.clone();
        match item.kind.as_str() {
            "Weapon" => {
                let free = self.weapons.iter().position(Option::is_none).unwrap_or(self.selected);
                let old = self.weapons[free].take();
                self.weapons[free] = self.bag.take(slot);
                if let Some(o) = old {
                    self.bag.slots[slot] = Some(o);
                }
                self.selected = free;
            }
            "Armor" if item.sub == "Body" => {
                let old = self.armor.take();
                self.armor = self.bag.take(slot);
                self.bag.slots[slot] = old;
            }
            "Armor" if item.sub == "Head" => {
                let old = self.head.take();
                self.head = self.bag.take(slot);
                self.bag.slots[slot] = old;
            }
            "Glider" => {
                let old = self.glider.take();
                self.glider = self.bag.take(slot);
                self.bag.slots[slot] = old;
            }
            "Orb" => {
                self.orb = stack.item.clone();
                return Ok(format!("{name} selected for throwing"));
            }
            "Ammo" if ARROWS.contains(&stack.item.as_str()) => {
                self.select_ammo(db, &stack.item);
                return Ok(format!("{name} selected for the bow"));
            }
            other => return Err(format!("{name} can't be equipped ({other})")),
        }
        Ok(format!("equipped {name}"))
    }

    /// Puts the loaded arrows back into the bag (switching weapon or arrow type).
    pub fn unload(&mut self, db: &Db) {
        if self.loaded > 0 {
            let (a, n) = (self.ammo.clone(), self.loaded);
            self.bag.add(&a, n, &*db.data);
            self.loaded = 0;
        }
    }

    /// With none of the chosen arrow type left (and nothing loaded), take whichever type there is.
    pub fn ensure_ammo(&mut self) {
        if self.loaded == 0
            && self.count(&self.ammo) == 0
            && let Some(a) = ARROWS.iter().find(|a| self.count(a) > 0)
        {
            self.ammo = (*a).to_owned();
        }
    }

    /// Use this arrow type with the bow (the loaded arrow goes back). False when there are none.
    pub fn select_ammo(&mut self, db: &Db, id: &str) -> bool {
        if self.count(id) == 0 {
            return false;
        }
        if self.ammo != id {
            self.unload(db);
            self.ammo = id.to_owned();
        }
        true
    }

    /// Next orb type in the bag, weakest to strongest (`orb_level`), wrapping (key 2).
    pub fn next_orb(&mut self, db: &Db) -> Option<String> {
        let mut kinds: Vec<(i64, String)> = Vec::new();
        for s in self.bag.slots.iter().flatten() {
            if let Some(i) = db.item(&s.item)
                && i.kind == "Orb"
                && !kinds.iter().any(|(_, k)| *k == s.item)
            {
                kinds.push((i.orb_level as i64, s.item.clone()));
            }
        }
        kinds.sort();
        let at = kinds.iter().position(|(_, k)| *k == self.orb);
        let next = match at {
            Some(i) if kinds.len() > 1 => kinds[(i + 1) % kinds.len()].1.clone(),
            None if !kinds.is_empty() => kinds[0].1.clone(),
            _ => return None,
        };
        self.orb = next.clone();
        Some(next)
    }

    /// Puts an equipped weapon back into the bag.
    pub fn unequip_weapon(&mut self, db: &Db, slot: usize) -> bool {
        let Some(s) = self.weapons.get_mut(slot).and_then(Option::take) else { return false };
        match self.bag.add_stack(s, &*db.data) {
            None => true,
            Some(back) => {
                self.weapons[slot] = Some(back);
                false
            }
        }
    }
}

/// The arrow list while R is held with a bow (combat.rs opens it, hud.rs shows it, the wheel moves `sel`
/// instead of switching weapon).
#[derive(Resource, Debug, Default)]
pub struct ArrowPicker {
    pub open: bool,
    pub sel: usize,
}

pub struct InventoryPlugin;

impl Plugin for InventoryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ArrowPicker>().add_systems(Startup, give_starting_items.after(crate::player::spawn_player)).add_systems(Update, (switch_weapon, switch_orb, equipment_effects).in_set(crate::Sim::Player).before(crate::player::locomotion));
    }
}

fn give_starting_items(mut commands: Commands, q: Query<Entity, With<Player>>) {
    for e in &q {
        // The player starts with an empty bag.
        commands.entity(e).insert(Inventory::new());
    }
}

fn switch_weapon(input: Res<PlayerInput>, time: Res<Time>, db: Res<Db>, mut log: ResMut<GameLog>, picker: Res<ArrowPicker>, mut q: Query<&mut Inventory, With<Player>>) {
    if input.weapon_scroll == 0 || picker.open {
        return;
    }
    for mut inv in &mut q {
        let n = WEAPON_SLOTS as i32;
        inv.selected = ((inv.selected as i32 + input.weapon_scroll).rem_euclid(n)) as usize;
        inv.unload(&db);
        let name = inv.weapon().and_then(|s| db.item(&s.item)).map_or("fists".to_owned(), |i| i.name.clone());
        log.say(&time, format!("weapon slot {}: {name}", inv.selected + 1));
    }
}

/// 2: the next orb type Q throws.
fn switch_orb(input: Res<PlayerInput>, time: Res<Time>, db: Res<Db>, mut log: ResMut<GameLog>, mut q: Query<&mut Inventory, With<Player>>) {
    if !input.orb_next {
        return;
    }
    for mut inv in &mut q {
        let line = match inv.next_orb(&db) {
            Some(s) => format!("orb: {} x{} (2: next)", db.item(&s).map_or(s.as_str(), |i| i.name.as_str()), inv.count(&s)),
            None => "orb: no other orb type in the bag".to_owned(),
        };
        log.say(&time, line);
    }
}

/// Glider equipped -> gliding allowed; `Carry` bonds in the party add carry weight; over the weight limit ->
/// no sprint/jump and half speed.
fn equipment_effects(db: Res<Db>, roster: Res<crate::creatures::Creatures>, mut q: Query<(&mut Inventory, &mut Locomotion), With<Player>>) {
    let bond_weight: f32 = roster
        .0
        .party()
        .iter()
        .flatten()
        .filter_map(|id| roster.0.get(*id))
        .filter_map(|p| db.creatures.get(&p.species)?.bond.as_ref().and_then(|b| db.bonds.get(b)))
        .map(|b| if let ck_core::data::BondKind::Carry { weight } = b.kind { weight } else { 0.0 })
        .sum();
    for (mut inv, mut loco) in &mut q {
        if inv.bond_weight != bond_weight {
            inv.bond_weight = bond_weight;
        }
        loco.has_glider = inv.glider.is_some() || loco.has_glider_debug;
        loco.overweight = inv.weight(&db) > inv.max_weight(&db);
    }
}
