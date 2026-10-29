//! World-projected HUD: creature nameplates (Lv box,
//! name, HP bar; red for wild creatures), the worker's current job icon and the hunger bubble, the interaction
//! prompt (key cap + label, hold progress), the capture percent box beside the aimed creature, the build-mode
//! panel, the item-get tiles and damage numbers. Projections are divided by `UiScale`, because the UI is
//! authored in 1920x1080 design pixels.

use bevy::prelude::*;

use crate::camera::MainCamera;
use crate::capture::CapturePreview;
use crate::combat::Combatant;
use crate::data::Db;
use crate::hud::text;
use crate::interact::Focus;
use crate::inventory::Inventory;
use crate::kinematic::Mover;
use crate::creatures::{CreatureActor, Creatures, Role};
use crate::player::{Health, Player};
use crate::ui::{Icons, col};

fn hex(s: &str) -> Color {
    Srgba::hex(s).map(Color::from).unwrap_or(Color::WHITE)
}

fn pt(p: f32) -> f32 {
    p * 4.0 / 3.0
}

#[derive(Component, Debug)]
struct Nameplate(Entity);

#[derive(Component, Debug)]
struct PlateName(Entity);

#[derive(Component, Debug)]
struct PlateLv(Entity);

#[derive(Component, Debug)]
struct PlateFill(Entity);

#[derive(Component, Debug)]
struct PlateIcon(Entity);

#[derive(Component, Debug)]
struct PlateMood(Entity);

#[derive(Component, Debug)]
struct Prompt;

#[derive(Component, Debug)]
struct PromptLabel;

#[derive(Component, Debug)]
struct PromptHold;

#[derive(Component, Debug)]
struct CaptureBox;

#[derive(Component, Debug)]
struct CaptureText;

#[derive(Component, Debug)]
struct BuildPanel;

#[derive(Component, Debug)]
struct BuildText;

#[derive(Component, Debug)]
struct GetTile(usize);

#[derive(Component, Debug)]
struct GetIcon(usize);

#[derive(Component, Debug)]
struct GetText(usize);

/// A floating damage number: world position, age.
#[derive(Component, Debug)]
struct DamageNumber {
    at: Vec3,
    age: f32,
}

const DAMAGE_NUMBER_S: f32 = 0.9;
/// Item-get tiles stay 5 s.
const GET_S: f32 = 5.0;

/// Recently gained items (merged per item while shown).
#[derive(Resource, Debug, Default)]
struct ItemGets {
    last: std::collections::HashMap<String, u32>,
    tiles: Vec<(String, u32, f32)>,
    primed: bool,
}

/// What a plate shows this frame.
struct PlateInfo {
    hp: f32,
    name: String,
    level: String,
    wild: bool,
    role: &'static str,
    task: Option<&'static str>,
    food: Option<(f32, f32)>,
}

pub struct HudMorePlugin;

impl Plugin for HudMorePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ItemGets>().init_resource::<PlateFrame>()
            .add_systems(Startup, spawn)
            .add_systems(Update, (ui_scale, nameplates, plate_parts, prompt, capture_box, build_panel, item_gets, damage_numbers).chain().after(crate::Sim::Last).after(crate::camera::follow));
    }
}

/// Author in 1920x1080 and scale to fit the window (the smaller of the two ratios).
fn ui_scale(windows: Query<&Window, With<bevy::window::PrimaryWindow>>, mut scale: ResMut<UiScale>) {
    if let Ok(w) = windows.single() {
        let s = (w.height() / 1080.0).min(w.width() / 1920.0).max(0.2);
        if (scale.0 - s).abs() > 1e-4 {
            scale.0 = s;
        }
    }
}

fn spawn(mut commands: Commands) {
    // Interaction prompt: [F] key cap with cyan brackets, label, hold fill.
    commands
        .spawn((Prompt, Node { position_type: PositionType::Absolute, flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(10.0), padding: UiRect::axes(px(8.0), px(4.0)), border: UiRect::right(px(1.0)), ..default() }, BackgroundColor(hex("11190EB3")), BorderColor::all(Color::WHITE), Visibility::Hidden, GlobalZIndex(3)))
        .with_children(|p| {
            p.spawn((Node { width: px(40.0), height: px(36.0), border: UiRect::horizontal(px(3.0)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, overflow: Overflow::clip(), ..default() }, BorderColor::all(hex("FFAC26")), BackgroundColor(hex("11190ECC")))).with_children(|k| {
                k.spawn((PromptHold, Node { position_type: PositionType::Absolute, left: px(0.0), top: px(0.0), width: percent(0.0), height: percent(100.0), ..default() }, BackgroundColor(hex("FFAC2699"))));
                k.spawn(text("F", pt(14.0)));
            });
            p.spawn((text("", pt(13.0)), PromptLabel));
        });
    // Capture percent box (upper right of the target, top-right corner radius 40).
    commands
        .spawn((CaptureBox, Node { position_type: PositionType::Absolute, width: px(214.0), padding: UiRect::all(px(8.0)), border: UiRect::left(px(3.0)), border_radius: BorderRadius { top_right: px(40.0), ..default() }, flex_direction: FlexDirection::Column, ..default() }, BackgroundColor(hex("00000080")), BorderColor::all(hex("FFC260")), Visibility::Hidden, GlobalZIndex(3)))
        .with_children(|c| {
            c.spawn((text("", pt(28.0)), CaptureText));
        });
    // Build-mode panel (bottom centre).
    commands
        .spawn((BuildPanel, Node { position_type: PositionType::Absolute, left: px(560.0), top: px(800.0), width: px(800.0), padding: UiRect::all(px(10.0)), border: UiRect::top(px(3.0)), ..default() }, BackgroundColor(hex("11190ED9")), BorderColor::all(hex("FFC260")), Visibility::Hidden))
        .with_children(|c| {
            c.spawn((text("", pt(12.0)), BuildText));
        });
    // Item-get row: up to 5 tiles centred at (960,860).
    commands
        .spawn((crate::hud::HudRoot, Node { position_type: PositionType::Absolute, left: px(560.0), top: px(780.0), width: px(800.0), justify_content: JustifyContent::Center, column_gap: px(6.0), ..default() }, Pickable::IGNORE))
        .with_children(|r| {
            for i in 0..5 {
                r.spawn((GetTile(i), Node { width: px(96.0), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, ..default() }, Visibility::Hidden)).with_children(|t| {
                    t.spawn((Node { width: px(76.0), height: px(76.0), border: UiRect::bottom(px(3.0)), ..default() }, BackgroundColor(hex("19191999")), BorderColor::all(hex("FFC260")))).with_children(|s| {
                        s.spawn((GetIcon(i), Node { width: px(70.0), height: px(70.0), margin: UiRect::all(px(3.0)), ..default() }, ImageNode::default()));
                    });
                    t.spawn((text("", pt(10.0)), GetText(i), TextLayout::justify(Justify::Center)));
                });
            }
        });
}

fn project(cam: (&Camera, &GlobalTransform), at: Vec3, scale: f32) -> Option<Vec2> {
    cam.0.world_to_viewport(cam.1, at).ok().map(|p| p / scale)
}

/// The job behind a worker task word (work.rs `verb`).
fn suit_of(kind: &str) -> Option<&'static str> {
    crate::data::JOBS.iter().copied().find(|s| crate::work::verb(s) == kind).or(match kind {
        "build" => Some("craft"),
        "ranch" => Some("ranch"),
        "transport" => Some("haul"),
        _ => None,
    })
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn nameplates(
    mut commands: Commands,
    scale: Res<UiScale>,
    roster: Res<Creatures>,
    db: Res<Db>,
    cam: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    actors: Query<(Entity, &Mover, &Health, &Combatant, &CreatureActor, &InheritedVisibility, Option<&crate::base::WorkerTask>)>,
    player: Query<&Mover, With<Player>>,
    mut plates: Query<(Entity, &Nameplate, &mut Node, &mut Visibility)>,
    mut frame: ResMut<PlateFrame>,
    ui: Res<crate::input::UiFocus>,
) {
    let info = &mut frame.0;
    info.clear();
    let (Ok(c), Ok(pm)) = (cam.single(), player.single()) else { return };
    let mut have = Vec::new();
    for (pe, plate, mut node, mut vis) in &mut plates {
        let Ok((_, m, hp, cb, a, v, task)) = actors.get(plate.0) else {
            commands.entity(pe).despawn();
            continue;
        };
        have.push(plate.0);
        let head = m.feet + Vec3::Y * (m.capsule.half_height * 2.0 + 0.55);
        match project(c, head, scale.0).filter(|_| !ui.0 && v.get() && m.feet.distance(pm.feet) < 30.0) {
            Some(p) => {
                *vis = Visibility::Inherited;
                node.left = px(p.x - 110.0);
                node.top = px(p.y - 52.0);
                let (name, level) = cb.name.rsplit_once(" Lv").map_or((cb.name.clone(), String::new()), |(n, l)| (n.to_owned(), l.to_owned()));
                info.insert(
                    pe,
                    PlateInfo {
                        hp: hp.hp / hp.max,
                        name,
                        level,
                        wild: matches!(a.role, Role::Wild { .. }),
                        role: match a.role {
                            Role::Wild { .. } => "",
                            Role::Companion => "  (party)",
                            Role::Worker => "  (worker)",
                        },
                        task: task.filter(|t| t.job.is_some() || t.carrying.is_some()).map(|t| t.kind),
                        food: roster.0.get(a.id).filter(|_| !matches!(a.role, Role::Wild { .. })).map(|p| (p.fullness, db.creatures[&p.species].stomach.max(1.0))),
                    },
                );
            }
            None => *vis = Visibility::Hidden,
        }
    }
    for (e, ..) in &actors {
        if have.contains(&e) {
            continue;
        }
        let plate = commands.spawn((Nameplate(e), Node { position_type: PositionType::Absolute, width: px(220.0), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, ..default() }, Visibility::Hidden)).id();
        commands.entity(plate).with_children(|c| {
            c.spawn((text("", pt(9.0)), PlateMood(plate), Node { padding: UiRect::axes(px(6.0), px(1.0)), ..default() }, BackgroundColor(Color::NONE), Visibility::Hidden));
            c.spawn(Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(4.0), ..default() }).with_children(|r| {
                r.spawn((PlateIcon(plate), Node { width: px(26.0), height: px(26.0), ..default() }, ImageNode::default(), Visibility::Hidden));
                r.spawn((Node { min_width: px(28.0), height: px(22.0), border: UiRect::all(px(1.0)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }, BackgroundColor(hex("28302399")), BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.45)))).with_children(|l| {
                    l.spawn((text("", pt(10.0)), PlateLv(plate)));
                });
                r.spawn((text("", pt(10.0)), PlateName(plate)));
            });
            c.spawn((Node { width: px(112.0), height: px(6.0), margin: UiRect::top(px(2.0)), ..default() }, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)))).with_children(|b| {
                b.spawn((PlateFill(plate), Node { width: percent(100.0), height: percent(100.0), ..default() }, BackgroundColor(hex("26BF73"))));
            });
        });
    }
}

#[derive(Resource, Default)]
struct PlateFrame(std::collections::HashMap<Entity, PlateInfo>);

#[allow(clippy::type_complexity)]
fn plate_parts(
    frame: Res<PlateFrame>,
    icons: Res<Icons>,
    mut names: Query<(&PlateName, &mut Text), (Without<PlateLv>, Without<PlateMood>)>,
    mut lvs: Query<(&PlateLv, &mut Text), (Without<PlateName>, Without<PlateMood>)>,
    mut fills: Query<(&PlateFill, &mut Node, &mut BackgroundColor), Without<PlateMood>>,
    mut work: Query<(&PlateIcon, &mut ImageNode, &mut Visibility), Without<PlateMood>>,
    mut moods: Query<(&PlateMood, &mut Text, &mut Visibility, &mut BackgroundColor), (Without<PlateName>, Without<PlateLv>, Without<PlateFill>, Without<PlateIcon>)>,
) {
    let f = &frame.0;
    for (p, mut t) in &mut names {
        if let Some(i) = f.get(&p.0) {
            let s = format!("{}{}", i.name, i.role);
            if t.0 != s {
                t.0 = s;
            }
        }
    }
    for (p, mut t) in &mut lvs {
        if let Some(i) = f.get(&p.0)
            && t.0 != i.level
        {
            t.0 = i.level.clone();
        }
    }
    for (p, mut n, mut bg) in &mut fills {
        if let Some(i) = f.get(&p.0) {
            n.width = percent(i.hp.clamp(0.0, 1.0) * 100.0);
            bg.0 = if i.wild { hex("FA3F3F") } else { hex("26BF73") };
        }
    }
    for (p, mut img, mut v) in &mut work {
        let icon = f.get(&p.0).and_then(|i| i.task).and_then(suit_of).and_then(|s| icons.get(&format!("work_{s}")));
        *v = if icon.is_some() { Visibility::Inherited } else { Visibility::Hidden };
        if let Some(h) = icon
            && img.image != h
        {
            img.image = h;
        }
    }
    // Hunger bubble: orange when hungry, red when empty.
    for (p, mut t, mut v, mut bg) in &mut moods {
        let (show, label, color) = match f.get(&p.0).and_then(|i| i.food) {
            Some((food, _)) if food <= 0.0 => (true, "Starving", hex("C30000CC")),
            Some((food, max)) if food < max * ck_core::consts::EAT_BELOW => (true, "Hungry", hex("C38A00CC")),
            _ => (false, "", Color::NONE),
        };
        *v = if show { Visibility::Inherited } else { Visibility::Hidden };
        if t.0 != label {
            t.0 = label.into();
        }
        bg.0 = color;
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn prompt(
    scale: Res<UiScale>,
    focus: Res<Focus>,
    input: Res<crate::input::PlayerInput>,
    placing: Res<crate::building::Placing>,
    ui: Res<crate::input::UiFocus>,
    cam: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    things: Query<(&GlobalTransform, Option<&crate::building::Construction>, Option<&crate::work::Producer>, Option<&crate::work::Farm>, Has<crate::crafting::Station>)>,
    mut q: Query<(&mut Node, &mut Visibility), With<Prompt>>,
    mut label: Query<&mut Text, With<PromptLabel>>,
    mut hold: Query<&mut Node, (With<PromptHold>, Without<Prompt>)>,
) {
    let (Ok(c), Ok((mut node, mut vis))) = (cam.single(), q.single_mut()) else { return };
    let target = focus.0.as_ref().filter(|_| placing.id.is_none() && !ui.0).and_then(|(e, p)| things.get(*e).ok().map(|t| (p, t)));
    let Some((p, (tf, cons, prod, farm, station))) = target else {
        *vis = Visibility::Hidden;
        return;
    };
    let Some(at) = project(c, tf.translation() + Vec3::Y * 1.2, scale.0) else {
        *vis = Visibility::Hidden;
        return;
    };
    *vis = Visibility::Inherited;
    node.left = px(at.x + 30.0);
    node.top = px(at.y - 20.0);
    let (name, what) = p.split_once(": ").unwrap_or((p.as_str(), ""));
    let (s, frac) = if let Some(cn) = cons {
        (format!("{name}\nHold: Build  {:.0}%", cn.work / cn.need * 100.0), Some(cn.work / cn.need))
    } else if let Some(pr) = prod {
        (format!("{name}\nHold: Work  (next {:.0}%)", pr.work / pr.need * 100.0), Some(pr.work / pr.need))
    } else if let Some(f) = farm {
        (format!("{name}\n{}", what.split(" (").next().unwrap_or(what)), Some(f.work / f.need().max(1.0)))
    } else if station {
        (format!("{name}\nTap: Open   Hold: Work"), None)
    } else {
        (if what.is_empty() { name.to_owned() } else { format!("{name}\n{what}") }, None)
    };
    if let Ok(mut t) = label.single_mut()
        && t.0 != s
    {
        t.0 = s;
    }
    if let Ok(mut h) = hold.single_mut() {
        let k = if input.interact_held { frac.unwrap_or(1.0) } else { 0.0 };
        h.width = percent(k.clamp(0.0, 1.0) * 100.0);
    }
}

fn capture_box(scale: Res<UiScale>, preview: Res<CapturePreview>, cam: Query<(&Camera, &GlobalTransform), With<MainCamera>>, mut q: Query<(&mut Node, &mut Visibility), With<CaptureBox>>, mut t: Query<&mut Text, With<CaptureText>>) {
    let (Ok(c), Ok((mut node, mut vis))) = (cam.single(), q.single_mut()) else { return };
    let target = preview.target.as_ref().zip(preview.target_at).filter(|_| preview.aiming);
    let Some(((name, bd, sneak), p)) = target.and_then(|(t, at)| project(c, at, scale.0).map(|p| (t, p))) else {
        *vis = Visibility::Hidden;
        return;
    };
    *vis = Visibility::Inherited;
    node.left = px(p.x + 8.0);
    node.top = px(p.y - 114.0);
    if let Ok(mut t) = t.single_mut() {
        t.0 = format!("{:.0}%\n{name}{}", (bd.result * 100.0).min(100.0), if *sneak { "\nBack Bonus!" } else { "" });
    }
}

fn build_panel(db: Res<Db>, placing: Res<crate::building::Placing>, dis: Res<crate::building::Dismantle>, player: Query<&Inventory, With<Player>>, mut q: Query<&mut Visibility, With<BuildPanel>>, mut t: Query<&mut Text, With<BuildText>>) {
    let Ok(mut vis) = q.single_mut() else { return };
    if dis.on {
        *vis = Visibility::Inherited;
        let hold = if dis.hold > 0.0 { format!("   {:.0}%", dis.hold / ck_core::consts::DISMANTLE_HOLD_S * 100.0) } else { String::new() };
        if let Ok(mut t) = t.single_mut() {
            t.0 = format!("DISMANTLE\n{}\n[Hold LMB] Dismantle{hold}   [X / Esc] Leave", dis.label);
        }
        return;
    }
    let (Some(id), Ok(inv)) = (&placing.id, player.single()) else {
        *vis = Visibility::Hidden;
        return;
    };
    *vis = Visibility::Inherited;
    let name = db.build(id).map_or(id.as_str(), |b| b.name.as_str());
    let mats = db.build(id).map_or(String::new(), |b| b.materials.iter().map(|(m, n)| format!("{} {}/{n}", db.item(m).map_or(m.as_str(), |i| i.name.as_str()), inv.count(m))).collect::<Vec<_>>().join("   "));
    let status = placing.ok.as_deref().map_or("Can place here".to_owned(), |w| format!("Cannot place: {w}"));
    if let Ok(mut t) = t.single_mut() {
        t.0 = format!("{name}\n{mats}\n{status}\n[LMB] Place   [R] Rotate   [RMB / Esc] Cancel");
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn item_gets(time: Res<Time>, db: Res<Db>, icons: Res<Icons>, mut gets: ResMut<ItemGets>, player: Query<&Inventory, With<Player>>, mut tiles: Query<(&GetTile, &mut Visibility)>, mut imgs: Query<(&GetIcon, &mut ImageNode)>, mut texts: Query<(&GetText, &mut Text)>) {
    let Ok(inv) = player.single() else { return };
    let mut now: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    for s in inv.bag.slots.iter().flatten() {
        *now.entry(s.item.clone()).or_default() += s.count;
    }
    if gets.primed {
        let mut added = Vec::new();
        for (item, n) in &now {
            let before = gets.last.get(item).copied().unwrap_or(0);
            if *n > before {
                added.push((item.clone(), n - before));
            }
        }
        for (item, add) in added {
            match gets.tiles.iter_mut().find(|t| t.0 == item) {
                Some(t) => {
                    t.1 += add;
                    t.2 = 0.0;
                }
                None => gets.tiles.push((item, add, 0.0)),
            }
        }
    }
    gets.primed = true;
    gets.last = now;
    let dt = time.delta_secs();
    for t in &mut gets.tiles {
        t.2 += dt;
    }
    gets.tiles.retain(|t| t.2 < GET_S);
    while gets.tiles.len() > 5 {
        gets.tiles.remove(0);
    }
    for (t, mut v) in &mut tiles {
        *v = if t.0 < gets.tiles.len() { Visibility::Inherited } else { Visibility::Hidden };
    }
    for (i, mut img) in &mut imgs {
        if let Some((item, ..)) = gets.tiles.get(i.0)
            && let Some(h) = icons.item(&db, item)
            && img.image != h
        {
            img.image = h;
        }
    }
    for (i, mut t) in &mut texts {
        if let Some((item, n, _)) = gets.tiles.get(i.0) {
            let s = format!("{} +{n}", db.item(item).map_or(item.as_str(), |x| x.name.as_str()));
            if t.0 != s {
                t.0 = s;
            }
        }
    }
}

/// Damage numbers rise from the hit target and fade (white = to enemies, red = to the player's side).
#[allow(clippy::type_complexity)]
fn damage_numbers(
    mut commands: Commands,
    time: Res<Time>,
    scale: Res<UiScale>,
    db: Res<crate::data::Db>,
    mut ev: MessageReader<crate::combat::Damaged>,
    cam: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    targets: Query<(&Mover, &Combatant)>,
    mut nums: Query<(Entity, &mut DamageNumber, &mut Node, &mut TextColor, &mut Visibility)>,
) {
    for d in ev.read() {
        let Ok((m, c)) = targets.get(d.target) else { continue };
        let ours = c.faction != crate::combat::Faction::Wild;
        let mut t = text(format!("{:.0}", d.amount), pt(if d.killed { 24.0 } else { 18.0 }));
        t.2 = TextColor(match d.slip {
            Some(s) => s.color(&db),
            None if ours => hex("FF3F3F"),
            None => col::TEXT,
        });
        commands.spawn((t, DamageNumber { at: m.feet + Vec3::Y * (m.capsule.half_height * 2.0 + 0.3), age: 0.0 }, Node { position_type: PositionType::Absolute, ..default() }, Visibility::Hidden, GlobalZIndex(4)));
    }
    let Ok(c) = cam.single() else { return };
    let dt = time.delta_secs();
    for (e, mut n, mut node, mut col, mut vis) in &mut nums {
        n.age += dt;
        if n.age > DAMAGE_NUMBER_S {
            commands.entity(e).despawn();
            continue;
        }
        match project(c, n.at + Vec3::Y * n.age * 1.2, scale.0) {
            Some(p) => {
                node.left = px(p.x - 14.0);
                node.top = px(p.y);
                col.0 = col.0.with_alpha(1.0 - n.age / DAMAGE_NUMBER_S);
                *vis = Visibility::Inherited;
            }
            None => *vis = Visibility::Hidden,
        }
    }
}
