//! Menu screens: inventory and status points (Tab), technology (T), build list (B), crafting station,
//! chest / feed box, and the Camp Core (party, storage, camp workers). Opening one frees the cursor.
//! Every action reports why it was refused.

use bevy::prelude::*;
use ck_core::creature::Place;

use crate::building::{Construction, Placing, Storage};
use crate::crafting::Station;
use crate::data::Db;
use crate::input::{PlayerInput, UiFocus};
use crate::interact::Interact;
use crate::inventory::Inventory;
use crate::log::GameLog;
use crate::creatures::Creatures;
use crate::player::{Health, Hunger, Player, Stamina};
use crate::progress::Progress;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Menu {
    None,
    Inventory,
    Tech,
    Build,
    Station(Entity),
    Storage(Entity),
    CampCore(Entity),
    Creature(u64),
    /// Repair Bench.
    Repair(Entity),
    /// Job Board: Job Priorities (tab 0) and the Assignment Board (tab 1).
    WorkBoard(Entity),
    /// Esc with nothing else open: display, sound, quit.
    Settings,
}

#[derive(Resource, Debug)]
pub struct MenuState {
    pub menu: Menu,
    pub dirty: bool,
    pub refresh: f32,
    pub root: Option<Entity>,
    pub msg: String,
    /// Where Close goes from a creature details page.
    pub back: Option<Menu>,
    /// Category tab of the build / station screen.
    pub tab: usize,
    /// Selected recipe (station) and quantity.
    pub sel: Option<String>,
    pub qty: u32,
    /// Selected creature in the Camp Core.
    pub sel_creature: Option<u64>,
    /// Assignment Board: the selected facility.
    pub sel_ent: Option<Entity>,
    /// The menu the current tree was built for (per-screen state resets when it changes).
    pub shown: Option<Menu>,
}

impl Default for MenuState {
    fn default() -> Self {
        Self { menu: Menu::None, dirty: true, refresh: 0.0, root: None, msg: String::new(), back: None, tab: 0, sel: None, qty: 1, sel_creature: None, sel_ent: None, shown: None }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Close,
    UseBag(usize),
    Unequip(usize),
    UnequipArmor,
    UnequipHead,
    UnequipGlider,
    Sort,
    Status(usize),
    Unlock(String),
    Build(String),
    Queue(Entity, String, u32),
    Cancel(Entity, usize),
    Collect(Entity),
    Store(Entity, usize),
    Take(Entity, usize),
    ToStorage(u64),
    ToParty(u64),
    ToCamp(u64),
    Details(u64),
    Equip(u64, String),
    /// Main-menu tab.
    Open(Menu),
    /// Category tab (build, station).
    Tab(usize),
    /// Pick a recipe at a station.
    Select(String),
    /// Change the quantity by this much.
    Qty(i32),
    TakeAll(Entity),
    /// Build menu: dismantle mode.
    Dismantle,
    /// Repair Bench: one item (by `base_tools::damaged` label) or all.
    Repair(Option<String>),
    /// Job Priorities: next priority number for (creature, job).
    CreaturePrio(u64, String),
    /// Assignment Board: pick a facility, then fix a creature to it / cancel a creature's assignment.
    PickFacility(Entity),
    FixCreature(u64, Entity),
    Unfix(u64),
    /// Settings: fullscreen (true) or windowed; sound on / off; quit the game.
    Fullscreen(bool),
    Sound(bool),
    Quit,
}

#[derive(Component, Debug, Clone)]
pub struct MenuButton(pub Action);

#[derive(Resource, Debug, Default)]
pub struct Clicks(pub Vec<Action>);

pub struct MenusPlugin;

impl Plugin for MenusPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MenuState>().init_resource::<Clicks>().add_systems(Update, (open_close, clicks, apply).chain().in_set(crate::Sim::Last)).add_systems(Update, (reset_screen, crate::menu_view::rebuild).chain().after(apply));
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn open_close(
    input: Res<PlayerInput>,
    mut st: ResMut<MenuState>,
    mut ev: MessageReader<Interact>,
    mut focus: ResMut<UiFocus>,
    (placing, carry, dis): (Res<Placing>, Res<crate::base_tools::Carry>, Res<crate::building::Dismantle>),
    stations: Query<(), With<Station>>,
    storages: Query<(), With<Storage>>,
    bases: Query<(), With<crate::building::BaseCamp>>,
    cons: Query<(), With<Construction>>,
    (benches, boards): (Query<(), With<crate::base_tools::RepairBench>>, Query<(), With<crate::base_tools::PriorityBoard>>),
) {
    let old = st.menu;
    let toggle = |st: &mut MenuState, m: Menu| st.menu = if st.menu == m { Menu::None } else { m };
    if input.menu_pressed {
        toggle(&mut st, Menu::Inventory);
    }
    if input.tech_pressed {
        toggle(&mut st, Menu::Tech);
    }
    if input.build_pressed && placing.id.is_none() {
        toggle(&mut st, Menu::Build);
    }
    // Esc closes the open screen; with nothing open (and not placing or dismantling, where Esc cancels
    // that) it opens Settings.
    if input.escape_pressed {
        st.menu = if old == Menu::None && placing.id.is_none() && !dis.on { Menu::Settings } else { Menu::None };
    }
    for Interact(e) in ev.read() {
        // Carrying a creature (or just put one down): F is for the creature, not for menus.
        if cons.contains(*e) || carry.creature.is_some() || carry.cooldown > 0.0 {
            continue;
        }
        if stations.contains(*e) {
            st.menu = Menu::Station(*e);
        } else if storages.contains(*e) {
            st.menu = Menu::Storage(*e);
        } else if bases.contains(*e) {
            st.menu = Menu::CampCore(*e);
        } else if benches.contains(*e) {
            st.menu = Menu::Repair(*e);
        } else if boards.contains(*e) {
            st.menu = Menu::WorkBoard(*e);
        }
    }
    if st.menu != old {
        st.dirty = true;
        st.msg.clear();
    }
    focus.0 = st.menu != Menu::None;
}

fn clicks(q: Query<(&Interaction, &MenuButton), Changed<Interaction>>, mut out: ResMut<Clicks>) {
    for (i, b) in &q {
        if *i == Interaction::Pressed {
            out.0.push(b.0.clone());
        }
    }
}

fn apply(world: &mut World) {
    let actions = std::mem::take(&mut world.resource_mut::<Clicks>().0);
    if actions.is_empty() {
        return;
    }
    let db = world.resource::<Db>().clone();
    let t = world.resource::<Time>().elapsed_secs();
    for a in actions {
        let msg = apply_one(world, &db, &a);
        let mut st = world.resource_mut::<MenuState>();
        st.dirty = true;
        if let Some(m) = msg {
            st.msg = m.clone();
            world.resource_mut::<GameLog>().push(t, m);
        }
    }
}

fn player_entity(world: &mut World) -> Option<Entity> {
    world.query_filtered::<Entity, With<Player>>().iter(world).next()
}

fn apply_one(world: &mut World, db: &Db, a: &Action) -> Option<String> {
    let pe = player_entity(world)?;
    match a {
        Action::Fullscreen(on) => {
            crate::settings::set_fullscreen(world, *on);
            Some(if *on { "Fullscreen".to_owned() } else { "Windowed".to_owned() })
        }
        Action::Sound(on) => {
            crate::settings::set_muted(world, !on);
            Some(if *on { "Sound on".to_owned() } else { "Sound off".to_owned() })
        }
        Action::Quit => {
            world.write_message(AppExit::Success);
            None
        }
        Action::Close => {
            let mut st = world.resource_mut::<MenuState>();
            st.menu = match st.menu {
                Menu::Creature(_) => st.back.take().unwrap_or(Menu::None),
                _ => Menu::None,
            };
            None
        }
        Action::Details(id) => {
            let mut st = world.resource_mut::<MenuState>();
            if matches!(st.menu, Menu::CampCore(_)) {
                st.sel_creature = Some(*id);
                return None;
            }
            if !matches!(st.menu, Menu::Creature(_)) {
                st.back = Some(st.menu);
            }
            st.menu = Menu::Creature(*id);
            st.msg.clear();
            None
        }
        Action::Equip(id, w) => {
            let name = db.skills.get(w).map_or(w.clone(), |z| z.name.clone());
            let mut roster = world.resource_mut::<Creatures>();
            let p = roster.0.get_mut(*id)?;
            Some(match p.toggle_equip(w) {
                Ok(true) => format!("equipped {name}"),
                Ok(false) => format!("unequipped {name}"),
                Err(e) => format!("rejected: {e}"),
            })
        }
        Action::Open(m) => {
            let mut st = world.resource_mut::<MenuState>();
            st.menu = *m;
            st.back = None;
            None
        }
        Action::Tab(i) => {
            let mut st = world.resource_mut::<MenuState>();
            st.tab = *i;
            st.sel = None;
            world.resource_mut::<crate::ui::ScrollMemory>().0.clear();
            None
        }
        Action::Select(r) => {
            let mut st = world.resource_mut::<MenuState>();
            st.sel = Some(r.clone());
            st.qty = 1;
            None
        }
        Action::Qty(d) => {
            let mut st = world.resource_mut::<MenuState>();
            st.qty = (st.qty as i32 + d).clamp(1, 999) as u32;
            None
        }
        Action::TakeAll(e) => {
            let n = world.get::<Storage>(*e)?.items.slots.len();
            let mut moved = 0;
            for i in 0..n {
                let Some(stack) = world.get_mut::<Storage>(*e)?.items.take(i) else { continue };
                match world.get_mut::<Inventory>(pe)?.bag.add_stack(stack, &*db.data) {
                    None => moved += 1,
                    Some(b) => {
                        world.get_mut::<Storage>(*e)?.items.slots[i] = Some(b);
                        return Some(format!("took {moved} stack(s); rejected the rest: bag full"));
                    }
                }
            }
            Some(format!("took {moved} stack(s)"))
        }
        Action::UseBag(slot) => {
            let stack = world.get::<Inventory>(pe)?.bag.slots.get(*slot).cloned().flatten()?;
            let item = db.item(&stack.item)?.clone();
            if item.kind == "Food" {
                let gain = item.food;
                world.get_mut::<Inventory>(pe)?.bag.remove(&stack.item, 1);
                let mut h = world.get_mut::<Hunger>(pe)?;
                h.value = (h.value + gain).min(h.max);
                Some(format!("ate {} (+{gain:.0} hunger)", item.name))
            } else {
                Some(world.get_mut::<Inventory>(pe)?.equip_from_bag(db, *slot).unwrap_or_else(|e| format!("rejected: {e}")))
            }
        }
        Action::Unequip(i) => Some(if world.get_mut::<Inventory>(pe)?.unequip_weapon(db, *i) { "unequipped".into() } else { "rejected: bag full".into() }),
        Action::UnequipArmor | Action::UnequipHead | Action::UnequipGlider => {
            let mut inv = world.get_mut::<Inventory>(pe)?;
            fn place<'a>(inv: &'a mut Inventory, a: &Action) -> &'a mut Option<ck_core::inventory::Stack> {
                match a {
                    Action::UnequipArmor => &mut inv.armor,
                    Action::UnequipHead => &mut inv.head,
                    _ => &mut inv.glider,
                }
            }
            let s = place(&mut inv, a).take()?;
            match inv.bag.add_stack(s, &*db.data) {
                None => Some("unequipped".into()),
                Some(back) => {
                    *place(&mut inv, a) = Some(back);
                    Some("rejected: bag full".into())
                }
            }
        }
        Action::Sort => {
            world.get_mut::<Inventory>(pe)?.bag.sort(&*db.data);
            None
        }
        Action::Status(i) => {
            let mut p = world.resource_mut::<Progress>();
            if p.status_points == 0 {
                return Some("rejected: no status points".into());
            }
            p.status_points -= 1;
            p.alloc[*i] += 1;
            match i {
                0 => {
                    let mut h = world.get_mut::<Health>(pe)?;
                    h.max += db.rules().point_hp;
                    h.hp += db.rules().point_hp;
                }
                1 => world.get_mut::<Stamina>(pe)?.max += db.rules().point_stamina,
                2 => world.get_mut::<crate::combat::Combatant>(pe)?.melee_attack += db.rules().point_attack,
                3 => world.get_mut::<Inventory>(pe)?.weight_points += 1,
                _ => {}
            }
            Some(["Max HP", "Max SP", "Attack", "Max Weight", "Work Speed"][*i].to_owned() + " +1")
        }
        Action::Unlock(id) => {
            let tech = db.tech(id)?.clone();
            let mut p = world.resource_mut::<Progress>();
            if p.unlocked.contains(&tech.id) {
                return Some("already unlocked".into());
            }
            if p.level < tech.level {
                return Some(format!("rejected: needs level {}", tech.level));
            }
            if let Some(r) = &tech.require
                && !p.unlocked.contains(r)
            {
                return Some(format!("rejected: needs {r} first"));
            }
            if p.tech_points < tech.cost {
                return Some(format!("rejected: needs {} technology points ({} left)", tech.cost, p.tech_points));
            }
            p.tech_points -= tech.cost;
            p.unlocked.insert(tech.id.clone());
            Some(format!("unlocked {}", tech.name))
        }
        Action::Build(id) => {
            let mut pl = world.resource_mut::<Placing>();
            pl.id = Some(id.clone());
            pl.rot = 0.0;
            world.resource_mut::<MenuState>().menu = Menu::None;
            Some(format!("placing {} (LMB place, R rotate, RMB/Esc cancel)", db.build(id).map_or(id.as_str(), |b| b.name.as_str())))
        }
        Action::Repair(which) => {
            let mut inv = world.get::<Inventory>(pe)?.clone();
            let free = world.resource::<crate::crafting::FreeCraft>().0;
            let line = crate::base_tools::repair(db, &mut inv, which.as_deref(), free);
            *world.get_mut::<Inventory>(pe)? = inv;
            Some(line)
        }
        Action::CreaturePrio(id, suit) => {
            let mut roster = world.resource_mut::<Creatures>();
            let p = roster.0.get_mut(*id)?;
            let v = crate::base_tools::next_prio(p.work_priority(suit));
            p.work_prio.insert(suit.clone(), v);
            let sp = db.creatures.get(&p.species).map_or(p.species.clone(), |s| s.name.clone());
            // New preferences: it chooses its job again.
            let mut q = world.query::<(&crate::creatures::CreatureActor, &mut crate::base::WorkerTask)>();
            for (a, mut t) in q.iter_mut(world) {
                if a.id == *id {
                    t.known.clear();
                }
            }
            Some(format!("{sp}: {} -> {}", crate::base_tools::suit_name(db, suit), if v == 0 { "off".to_owned() } else { format!("priority {v}") }))
        }
        Action::PickFacility(e) => {
            world.resource_mut::<MenuState>().sel_ent = Some(*e);
            None
        }
        Action::FixCreature(id, e) => {
            let species = world.resource::<Creatures>().0.get(*id)?.species.clone();
            let b = world.get::<crate::building::Building>(*e)?.clone();
            let (blueprint, ranch) = (world.get::<Construction>(*e).is_some(), world.get::<crate::base::Ranch>(*e).is_some());
            let mut q = world.query::<(&crate::creatures::CreatureActor, &mut crate::base::WorkerTask)>();
            let fixed_there = q.iter(world).filter(|(_, t)| t.forced == Some(*e)).count();
            match crate::base_tools::check_fixed(db, &species, &b, blueprint, ranch, fixed_there) {
                Ok(()) => {
                    for (a, mut t) in q.iter_mut(world) {
                        if a.id == *id {
                            t.forced = Some(*e);
                            t.job = None;
                        }
                    }
                    Some(format!("assigned to the {}", b.name))
                }
                Err(why) => Some(format!("rejected: {why}")),
            }
        }
        Action::Unfix(id) => {
            let mut q = world.query::<(&crate::creatures::CreatureActor, &mut crate::base::WorkerTask)>();
            for (a, mut t) in q.iter_mut(world) {
                if a.id == *id {
                    t.forced = None;
                }
            }
            Some("Cancel Assignment: back to Automatic Work Mode".to_owned())
        }
        Action::Dismantle => {
            let mut d = world.resource_mut::<crate::building::Dismantle>();
            d.on = true;
            world.resource_mut::<MenuState>().menu = Menu::None;
            Some("dismantle mode: hold LMB on an object (X / Esc to leave)".to_owned())
        }
        Action::Queue(e, r, n) => {
            let mut inv = world.get::<Inventory>(pe)?.clone();
            let mut st = world.get::<Station>(*e)?.clone();
            let free = world.resource::<crate::crafting::FreeCraft>().0;
            let res = crate::crafting::queue(db, &mut st, &mut inv, r, *n, free);
            if res.is_ok() {
                *world.get_mut::<Inventory>(pe)? = inv;
                *world.get_mut::<Station>(*e)? = st;
            }
            Some(match res {
                Ok(()) => format!("queued {} x{n} (hold F at the station to work)", db.recipe(r).and_then(|x| db.item(&x.product)).map_or(r.as_str(), |i| i.name.as_str())),
                Err(why) => format!("rejected: {why}"),
            })
        }
        Action::Cancel(e, i) => {
            let mut inv = world.get::<Inventory>(pe)?.clone();
            let mut st = world.get::<Station>(*e)?.clone();
            let lost = crate::crafting::cancel(db, &mut st, &mut inv, *i);
            *world.get_mut::<Inventory>(pe)? = inv;
            *world.get_mut::<Station>(*e)? = st;
            Some(if lost.is_empty() { "cancelled; materials returned".into() } else { format!("cancelled; bag full, lost {lost:?}") })
        }
        Action::Collect(e) => {
            let mut inv = world.get::<Inventory>(pe)?.clone();
            let mut st = world.get::<Station>(*e)?.clone();
            let got = crate::crafting::collect(db, &mut st, &mut inv);
            *world.get_mut::<Inventory>(pe)? = inv;
            *world.get_mut::<Station>(*e)? = st;
            Some(if got.is_empty() { "nothing collected (bag full?)".into() } else { format!("collected {}", got.iter().map(|(i, n)| format!("{} x{n}", db.item(i).map_or(i.as_str(), |x| x.name.as_str()))).collect::<Vec<_>>().join(", ")) })
        }
        Action::Store(e, slot) => {
            let stack = world.get_mut::<Inventory>(pe)?.bag.take(*slot)?;
            let food_only = world.get::<Storage>(*e)?.food_only;
            if food_only && db.item(&stack.item).is_none_or(|i| i.kind != "Food") {
                world.get_mut::<Inventory>(pe)?.bag.slots[*slot] = Some(stack);
                return Some("rejected: the feed box only takes food".into());
            }
            let back = world.get_mut::<Storage>(*e)?.items.add_stack(stack, &*db.data);
            if let Some(b) = back {
                world.get_mut::<Inventory>(pe)?.bag.slots[*slot] = Some(b);
                return Some("rejected: storage full".into());
            }
            None
        }
        Action::Take(e, slot) => {
            let stack = world.get_mut::<Storage>(*e)?.items.take(*slot)?;
            if let Some(b) = world.get_mut::<Inventory>(pe)?.bag.add_stack(stack, &*db.data) {
                world.get_mut::<Storage>(*e)?.items.slots[*slot] = Some(b);
                return Some("rejected: bag full".into());
            }
            None
        }
        Action::ToStorage(id) | Action::ToParty(id) | Action::ToCamp(id) => {
            let place = match a {
                Action::ToStorage(_) => Place::Storage,
                Action::ToCamp(_) => Place::Camp,
                _ => {
                    let party = world.resource::<Creatures>().0.party();
                    match party.iter().position(Option::is_none) {
                        Some(i) => Place::Party(i),
                        None => return Some("rejected: party is full (5)".into()),
                    }
                }
            };
            if place == Place::Camp {
                let limit = crate::base::worker_limit(world, db);
                let n = world.resource::<Creatures>().0.in_place(Place::Camp).len();
                if n >= limit {
                    let level = world.query::<&crate::building::BaseCamp>().iter(world).map(|b| b.level).max().unwrap_or(1);
                    return Some(format!("rejected: camp Lv {level} allows {limit} worker(s); finish the camp goals (camp panel) to level up"));
                }
            }
            let res = world.resource_mut::<Creatures>().0.move_to(*id, place);
            // A summoned creature that leaves the party goes back into its orb first.
            let mut party = world.resource::<crate::companion::Party>().clone();
            let roster = world.resource::<Creatures>();
            let moved_out = party.active_id.is_some_and(|a| !matches!(roster.0.get(a).map(|p| p.place), Some(Place::Party(_))));
            if moved_out && let Some(e) = party.active_entity.take() {
                party.active_id = None;
                world.despawn(e);
                *world.resource_mut::<crate::companion::Party>() = party;
            }
            Some(match res {
                Ok(()) => format!("moved to {place:?}"),
                Err(e) => format!("rejected: {e}"),
            })
        }
    }
}

/// Per-screen state (tab, selection, quantity, scroll) starts fresh when another screen opens.
fn reset_screen(mut st: ResMut<MenuState>, mut mem: ResMut<crate::ui::ScrollMemory>) {
    let same = match (st.shown, st.menu) {
        (Some(Menu::Creature(_)), Menu::Creature(_)) => true,
        (Some(a), b) => a == b,
        (None, b) => b == Menu::None,
    };
    if same {
        return;
    }
    if !matches!(st.menu, Menu::CampCore(_)) {
        st.sel_creature = None;
    }
    st.tab = 0;
    st.sel = None;
    st.qty = 1;
    st.shown = Some(st.menu);
    st.dirty = true;
    mem.0.clear();
}
