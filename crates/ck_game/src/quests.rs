//! Missions tracker (left column, under the level badge), the camp panel (camp level, workers, the goals for
//! the next level) and the level-up banner. The missions are world.ron `missions`, done in order; each
//! has a goal the tracker checks every frame.

use bevy::prelude::*;
use ck_core::data::Goal;

use crate::building::{Building, Construction};
use crate::data::Db;
use crate::hud::text;
use crate::inventory::Inventory;
use crate::log::{GameLog, Say};
use crate::creatures::Creatures;
use crate::player::{Hunger, Player};
use crate::progress::Progress;
use ck_core::creature::Place;

#[derive(Resource, Debug, Default)]
pub struct Missions {
    /// Seconds left on the "complete" flash.
    flash: f32,
    flash_title: String,
    last_hunger: Option<f32>,
}

#[derive(Component, Debug)]
struct MissionText;

#[derive(Component, Debug)]
struct MissionPanel;

#[derive(Component, Debug)]
struct BaseText;

#[derive(Component, Debug)]
struct BasePanel;

#[derive(Component, Debug)]
struct Banner;

#[derive(Component, Debug)]
struct BannerText;

/// Level-up banner state: what was seen last, seconds left.
#[derive(Resource, Debug, Default)]
struct BannerState {
    base: Option<u32>,
    player: Option<u32>,
    left: f32,
}

/// How long the level-up banner shows.
const BANNER_S: f32 = 4.0;

pub struct QuestPlugin;

impl Plugin for QuestPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Missions>().init_resource::<BannerState>().add_systems(Startup, spawn).add_systems(Update, (track, show, base_panel, banner).chain().after(crate::Sim::Last));
    }
}

fn hex(s: &str) -> Color {
    Srgba::hex(s).map(Color::from).unwrap_or(Color::WHITE)
}

fn spawn(mut commands: Commands) {
    // Left column under the vitals card: mission tracker, then the camp panel, each with an accent bar.
    let col = commands.spawn((crate::hud::HudRoot, Node { position_type: PositionType::Absolute, left: px(20.0), top: px(160.0), width: px(420.0), flex_direction: FlexDirection::Column, row_gap: px(8.0), ..default() })).id();
    let tracker = |c: &mut Commands, marker_text: bool| {
        let panel = c.spawn((Node { padding: UiRect::new(px(12.0), px(10.0), px(6.0), px(8.0)), border: UiRect::left(px(3.0)), flex_direction: FlexDirection::Column, ..default() }, BackgroundColor(hex("11190EA6")), BorderColor::all(hex("FFC260")), ChildOf(col))).id();
        let t = c.spawn((text("", 16.0), ChildOf(panel))).id();
        if marker_text {
            c.entity(panel).insert(MissionPanel);
            c.entity(t).insert(MissionText);
        } else {
            c.entity(panel).insert((BasePanel, Visibility::Hidden));
            c.entity(t).insert(BaseText);
        }
    };
    tracker(&mut commands, true);
    tracker(&mut commands, false);
    // Level-up banner: 864x46 at y 112, cyan-to-red bar with a 4 px bottom line.
    commands
        .spawn((Banner, Node { position_type: PositionType::Absolute, left: px(528.0), top: px(112.0), width: px(864.0), height: px(46.0), border: UiRect::bottom(px(4.0)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }, BackgroundGradient::from(LinearGradient::to_right(vec![ColorStop::auto(hex("FFAC26CC")), ColorStop::auto(hex("11190ECC")), ColorStop::auto(hex("CD1000CC"))])), BorderColor::all(hex("CD0C00")), Visibility::Hidden, GlobalZIndex(5)))
        .with_children(|b| {
            b.spawn((text("", 29.0), BannerText));
        });
}

/// The goal's progress: (have, need), or None for yes/no goals.
fn progress_of(goal: &Goal, inv: &Inventory, prog: &Progress, owned: u32, workers: u32) -> Option<(u32, u32)> {
    match goal {
        Goal::Have(item, n) => Some((inv.count(item), *n)),
        Goal::Capture(n) => Some((owned, *n)),
        Goal::Workers(n) => Some((workers, *n)),
        Goal::Level(n) => Some((prog.level, *n)),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn track(
    time: Res<Time>,
    db: Res<Db>,
    mut prog: ResMut<Progress>,
    mut ms: ResMut<Missions>,
    roster: Res<Creatures>,
    mut log: ResMut<GameLog>,
    player: Query<(&Inventory, &Hunger), With<Player>>,
    buildings: Query<(&Building, Has<Construction>)>,
) {
    let Ok((inv, hunger)) = player.single() else { return };
    ms.flash = (ms.flash - time.delta_secs()).max(0.0);
    let ate = ms.last_hunger.is_some_and(|h| hunger.value > h + 0.5);
    ms.last_hunger = Some(hunger.value);
    let built = |id: &str| buildings.iter().any(|(b, c)| !c && b.id.eq_ignore_ascii_case(id));
    let owned = roster.0.all.values().filter(|p| p.owned).count() as u32;
    let workers = roster.0.all.values().filter(|p| p.owned && p.place == Place::Camp).count() as u32;
    let worn = |item: &str| [&inv.armor, &inv.head, &inv.glider].iter().any(|s| s.as_ref().is_some_and(|s| s.item.eq_ignore_ascii_case(item)));
    // Several can complete in one frame (wood already in the bag, ...).
    while let Some(m) = db.world.missions.get(prog.mission) {
        let done = match &m.goal {
            Goal::Build(id) => built(id),
            Goal::Unlock(t) => prog.unlocked.contains(t),
            // Crafted: it is in the bag (or, for an orb, already used on a capture).
            Goal::Craft(item) => inv.count(item) > 0 || (db.item(item).is_some_and(|i| i.kind == "Orb") && owned > 0),
            Goal::Eat => ate,
            Goal::Equip(item) => worn(item),
            Goal::SpendPoint => prog.alloc.iter().sum::<u32>() > 0,
            g => progress_of(g, inv, &prog, owned, workers).is_some_and(|(have, need)| have >= need),
        };
        if !done {
            break;
        }
        log.say(&time, format!("mission complete: {}", m.title));
        ms.flash = 3.0;
        ms.flash_title = m.title.clone();
        prog.mission += 1;
    }
}

#[allow(clippy::too_many_arguments)]
fn show(db: Res<Db>, prog: Res<Progress>, ms: Res<Missions>, roster: Res<Creatures>, player: Query<&Inventory, With<Player>>, mut t: Query<&mut Text, With<MissionText>>, mut panel: Query<(&mut BorderColor, &mut BackgroundColor), With<MissionPanel>>, free: Res<crate::crafting::FreeCraft>) {
    let Ok((mut border, mut bg)) = panel.single_mut() else { return };
    let Ok(mut t) = t.single_mut() else { return };
    let Ok(inv) = player.single() else { return };
    let mut lines = Vec::new();
    if db.easy {
        lines.push("SANDBOX (--sandbox): costs capped at 1, max level".to_owned());
    }
    if free.0 {
        lines.push("FREE CRAFTING ON (F2): missing materials are ignored".to_owned());
    }
    if ms.flash > 0.0 {
        lines.push(format!("MISSION COMPLETE: {}", ms.flash_title));
    }
    *border = BorderColor::all(if ms.flash > 0.0 { crate::ui::col::GOOD } else { crate::ui::col::ACCENT });
    bg.0 = if ms.flash > 0.0 { crate::ui::col::GOOD_BG } else { crate::ui::col::PANEL };
    let Some(m) = db.world.missions.get(prog.mission) else {
        lines.push("All missions done".into());
        t.0 = lines.join("\n");
        return;
    };
    lines.push(format!("MISSION {}/{}  {}", prog.mission + 1, db.world.missions.len(), m.title));
    lines.push(m.text.clone());
    let owned = roster.0.all.values().filter(|p| p.owned).count() as u32;
    let workers = roster.0.all.values().filter(|p| p.owned && p.place == Place::Camp).count() as u32;
    if let Some((have, need)) = progress_of(&m.goal, inv, &prog, owned, workers) {
        lines.push(format!("  {} / {need}", have.min(need)));
    }
    // Technology gates: say which level unlocks what the mission needs.
    let needs_tech = match &m.goal {
        Goal::Craft(item) => db.technologies.iter().find(|t| t.recipes.iter().any(|r| db.recipe(r).is_some_and(|r| r.product == *item))),
        Goal::Build(id) => db.technologies.iter().find(|t| t.builds.contains(id)),
        Goal::Unlock(id) => db.tech(id),
        _ => None,
    };
    if let Some(tc) = needs_tech.filter(|tc| !prog.unlocked.contains(&tc.id)) {
        if tc.level > prog.level {
            lines.push(format!("  (unlocks at Lv {}: {}; you are Lv {})", tc.level, tc.name, prog.level));
        } else {
            lines.push(format!("  (unlock {} in Technology: T)", tc.name));
        }
    }
    t.0 = lines.join("\n");
}

/// Camp panel: level, workers / limit, and the next level's goals with ticks (shown once a Camp Core stands).
#[allow(clippy::type_complexity)]
fn base_panel(db: Res<Db>, roster: Res<Creatures>, bases: Query<&crate::building::BaseCamp>, built: Query<&Building, Without<Construction>>, mut panel: Query<&mut Visibility, With<BasePanel>>, mut t: Query<&mut Text, With<BaseText>>) {
    let (Ok(mut vis), Ok(mut t)) = (panel.single_mut(), t.single_mut()) else { return };
    let Some(bc) = bases.iter().next() else {
        *vis = Visibility::Hidden;
        return;
    };
    *vis = Visibility::Inherited;
    let workers = roster.0.in_place(Place::Camp).len() as u32;
    let ids: Vec<String> = built.iter().map(|b| b.id.clone()).collect();
    let mut lines = vec![format!("CAMP  Lv {}    Workers {workers} / {}", bc.level, crate::base::max_workers(&db, bc.level))];
    let missions = crate::base::base_missions(&db, bc.level, workers, &ids);
    if missions.is_empty() || !crate::base::has_level(&db, bc.level + 1) {
        lines.push("Max camp level".into());
    } else {
        lines.push(format!("To reach Lv {} (allows {} workers):", bc.level + 1, crate::base::max_workers(&db, bc.level + 1)));
        for m in &missions {
            let tick = if m.have >= m.need { "[x]" } else { "[ ]" };
            lines.push(format!("{tick} {}  {}/{}", m.what, m.have.min(m.need), m.need));
        }
        lines.push("(send creatures to work: F at the Camp Core)".into());
    }
    let s = lines.join("
");
    if t.0 != s {
        t.0 = s;
    }
}

/// Level-up banner for the player level and the camp level.
#[allow(clippy::too_many_arguments)]
fn banner(time: Res<Time>, ui: Res<crate::input::UiFocus>, prog: Res<Progress>, mut st: ResMut<BannerState>, bases: Query<&crate::building::BaseCamp>, mut b: Query<&mut Visibility, With<Banner>>, mut t: Query<&mut Text, With<BannerText>>) {
    let base = bases.iter().next().map(|b| b.level);
    let mut msg = None;
    if st.player.is_some_and(|l| prog.level > l) {
        msg = Some(format!("LEVEL UP!   Lv {}", prog.level));
    }
    if let (Some(old), Some(new)) = (st.base, base)
        && new > old
    {
        msg = Some(format!("CAMP LEVEL UP!   Lv {new}"));
    }
    st.player = Some(prog.level);
    st.base = base;
    if let Some(m) = msg
        && let Ok(mut t) = t.single_mut()
    {
        t.0 = m;
        st.left = BANNER_S;
    }
    st.left = (st.left - time.delta_secs()).max(0.0);
    if let Ok(mut v) = b.single_mut() {
        *v = if st.left > 0.0 && !ui.0 { Visibility::Inherited } else { Visibility::Hidden };
    }
}
