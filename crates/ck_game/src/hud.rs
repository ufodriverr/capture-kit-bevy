//! Screen-fixed HUD, laid out in 1920x1080 design pixels: the vitals card top left (level, HP, stamina,
//! hunger, EXP), compass and clock top right with the event log under them, the party column on the
//! left edge, and the hotbar at the bottom centre (orb, weapon with ammo and durability, bond skill). The
//! F1 inspector readout sits beside the vitals card.

use bevy::prelude::*;
use bevy::text::FontSize;

use crate::camera::CameraRig;
use crate::companion::Party;
use crate::data::Db;
use crate::inventory::Inventory;
use crate::kinematic::Mover;
use crate::log::GameLog;
use crate::creatures::{CreatureActor, Creatures, WorldClock};
use crate::partner::PartnerState;
use crate::player::{Health, Hunger, Locomotion, Player, Stamina};
use crate::progress::Progress;
use crate::ui::{Icons, col};

/// Text with the HUD's outline-like shadow. `size` is in points: px = pt x 4/3.
pub fn text(s: impl Into<String>, size: f32) -> (Text, TextFont, TextColor, TextShadow) {
    tinted(s, size, col::TEXT)
}

pub fn tinted(s: impl Into<String>, size: f32, color: Color) -> (Text, TextFont, TextColor, TextShadow) {
    (Text::new(s), TextFont { font_size: FontSize::Px(size), ..default() }, TextColor(color), TextShadow { offset: Vec2::splat(1.5), color: Color::srgba(0.0, 0.0, 0.0, 0.75) })
}

fn pt(p: f32) -> f32 {
    p * 4.0 / 3.0
}

fn abs(x: f32, y: f32, w: f32, h: f32) -> Node {
    Node { position_type: PositionType::Absolute, left: px(x), top: px(y), width: px(w), height: px(h), ..default() }
}

fn hex(s: &str) -> Color {
    Srgba::hex(s).map(Color::from).unwrap_or(Color::WHITE)
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
enum Fill {
    Hp,
    Hunger,
    Exp,
    Durability,
    PartyHp,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
enum Label {
    /// The arrow list while R is held.
    ArrowList,
    HpNum,
    Level,
    Exp,
    Clock,
    OrbName,
    OrbCount,
    WeaponName,
    Ammo,
    Slots,
    PartyName,
    PartyLv,
    PartnerName,
    PartnerCd,
    Readout,
    Log,
    Hints,
}

#[derive(Component, Debug, Clone, Copy)]
enum Img {
    Orb,
    Weapon,
    /// Party carousel: -1 left, 0 centre, +1 right.
    Party(i32),
}

#[derive(Component, Debug)]
struct PartyRing(i32);

#[derive(Component, Debug)]
struct PartnerCircle;

#[derive(Component, Debug)]
struct StaminaDot(usize);

#[derive(Component, Debug)]
struct StaminaArc;

#[derive(Component, Debug)]
struct CompassTick(i32);

#[derive(Component, Debug)]
struct CompassBase;

/// A screen-fixed HUD group; hidden while a menu has the cursor.
#[derive(Component, Debug)]
pub struct HudRoot;

const STAMINA_DOTS: usize = 22;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_hud).add_systems(Update, (gauges, labels, images, party_carousel, partner_circle, stamina_arc, compass, hide_in_menus).after(crate::Sim::Last));
    }
}

fn spawn_hud(mut commands: Commands, icons: Res<Icons>) {
    let root = commands.spawn((Name::new("Hud"), HudRoot, Node { width: px(1920.0), height: px(1080.0), position_type: PositionType::Absolute, ..default() }, Pickable::IGNORE)).id();
    let spawn_text = |c: &mut Commands, s: &str, size: f32, node: Node, l: Label| {
        c.spawn((text(s, pt(size)), node, l, ChildOf(root)));
    };
    let panel = |c: &mut Commands, x: f32, y: f32, w: f32, h: f32| {
        c.spawn((BackgroundColor(col::PANEL), BorderColor::all(col::ACCENT.with_alpha(0.55)), Node { border: UiRect::all(px(1.0)), border_radius: BorderRadius::all(px(10.0)), ..abs(x, y, w, h) }, ChildOf(root)));
    };
    // Crosshair: a small open ring on its own full-window root (the 1920x1080 canvas sits in the top-left
    // corner when the window isn't 16:9, and the crosshair must stay on the camera's centre line).
    commands.spawn((HudRoot, Node { position_type: PositionType::Absolute, left: percent(50.0), top: percent(50.0), width: px(10.0), height: px(10.0), margin: UiRect { left: px(-5.0), top: px(-5.0), ..default() }, border: UiRect::all(px(2.0)), border_radius: BorderRadius::MAX, ..default() }, BorderColor::all(Color::srgba(1.0, 0.95, 0.85, 0.9)), Pickable::IGNORE));

    // Top left: the vitals card. Level medallion, HP, stamina segments, hunger, EXP.
    panel(&mut commands, 20.0, 20.0, 420.0, 124.0);
    commands.spawn((BackgroundColor(hex("2A3A22")), BorderColor::all(col::ACCENT), Node { border: UiRect::all(px(3.0)), border_radius: BorderRadius::MAX, ..abs(34.0, 34.0, 84.0, 84.0) }, ChildOf(root))).with_children(|b| {
        b.spawn((tinted("LEVEL", pt(7.0), col::ACCENT), Node { position_type: PositionType::Absolute, left: px(0.0), top: px(10.0), width: px(78.0), justify_content: JustifyContent::Center, ..default() }, TextLayout::justify(Justify::Center)));
        b.spawn((text("1", pt(22.0)), Label::Level, Node { position_type: PositionType::Absolute, left: px(0.0), top: px(26.0), width: px(78.0), justify_content: JustifyContent::Center, ..default() }, TextLayout::justify(Justify::Center)));
    });
    icon_at(&mut commands, root, &icons, "stat_hp", 132.0, 34.0, 22.0);
    bar_at(&mut commands, root, 160.0, 36.0, 264.0, 18.0, hex("6FBF4A"), Fill::Hp);
    spawn_text(&mut commands, "", 11.0, Node { position_type: PositionType::Absolute, left: px(168.0), top: px(36.0), ..default() }, Label::HpNum);
    // Stamina: a row of segments, always shown.
    commands.spawn((StaminaArc, abs(160.0, 62.0, 264.0, 8.0), ChildOf(root))).with_children(|c| {
        let w = 264.0 / STAMINA_DOTS as f32;
        for i in 0..STAMINA_DOTS {
            c.spawn((StaminaDot(i), Node { border_radius: BorderRadius::all(px(2.0)), ..abs(i as f32 * w, 0.0, w - 2.0, 8.0) }, BackgroundColor(Color::WHITE)));
        }
    });
    icon_at(&mut commands, root, &icons, "grilled_meat", 132.0, 74.0, 22.0);
    bar_at(&mut commands, root, 160.0, 80.0, 264.0, 8.0, hex("E08A3C"), Fill::Hunger);
    bar_at(&mut commands, root, 132.0, 102.0, 292.0, 4.0, col::ACCENT, Fill::Exp);
    spawn_text(&mut commands, "", 9.0, Node { position_type: PositionType::Absolute, left: px(132.0), top: px(110.0), ..default() }, Label::Exp);

    // Top right: compass strip (420 px for 180 degrees) with the camp marker, the clock under it.
    commands.spawn((BackgroundColor(col::PANEL), Node { overflow: Overflow::clip(), border_radius: BorderRadius::all(px(8.0)), ..abs(COMPASS_X, 20.0, COMPASS_W, 34.0) }, ChildOf(root))).with_children(|c| {
        for i in 0..24 {
            let deg = i * 15;
            let s = match deg {
                0 => "N",
                90 => "E",
                180 => "S",
                270 => "W",
                45 => "NE",
                135 => "SE",
                225 => "SW",
                315 => "NW",
                _ => ".",
            };
            let size = if deg % 90 == 0 { pt(13.0) } else if deg % 45 == 0 { pt(9.0) } else { pt(9.0) };
            c.spawn((text(s, size), CompassTick(deg), Node { position_type: PositionType::Absolute, top: px(if deg % 45 == 0 { 4.0 } else { 2.0 }), ..default() }, Visibility::Hidden));
        }
        c.spawn((tinted("camp", pt(9.0), col::ACCENT), CompassBase, Node { position_type: PositionType::Absolute, top: px(18.0), ..default() }, Visibility::Hidden));
    });
    commands.spawn((abs(COMPASS_X + COMPASS_W / 2.0 - 1.0, 50.0, 2.0, 10.0), BackgroundColor(col::ACCENT), ChildOf(root)));
    spawn_text(&mut commands, "", 11.0, Node { position_type: PositionType::Absolute, left: px(COMPASS_X), top: px(62.0), width: px(COMPASS_W), justify_content: JustifyContent::Center, ..default() }, Label::Clock);

    // Left edge, middle: the party column. Previous slot above, the selected one large, the next below.
    for (k, cy, size) in [(-1, 600.0, 52.0), (1, 800.0, 52.0), (0, 700.0, 88.0)] {
        let ring = size + 14.0;
        commands.spawn((PartyRing(k), Node { border: UiRect::all(px(if k == 0 { 3.0 } else { 2.0 })), border_radius: BorderRadius::all(px(14.0)), ..abs(70.0 - ring / 2.0, cy - ring / 2.0, ring, ring) }, BorderColor::all(col::ACCENT), BackgroundColor(col::PANEL), ChildOf(root)));
        commands.spawn((Img::Party(k), abs(70.0 - size / 2.0, cy - size / 2.0, size, size), ImageNode::default(), ChildOf(root)));
    }
    commands.spawn((text("1", pt(10.0)), key_cap(), Node { position_type: PositionType::Absolute, left: px(110.0), top: px(586.0), ..key_cap_node() }, ChildOf(root)));
    commands.spawn((text("3", pt(10.0)), key_cap(), Node { position_type: PositionType::Absolute, left: px(110.0), top: px(788.0), ..key_cap_node() }, ChildOf(root)));
    commands.spawn((text("E", pt(10.0)), key_cap(), Node { position_type: PositionType::Absolute, left: px(126.0), top: px(718.0), ..key_cap_node() }, ChildOf(root)));
    // Name plate beside the selected creature: Lv, name, HP bar.
    commands.spawn((BackgroundColor(col::PANEL), Node { border_radius: BorderRadius::all(px(8.0)), ..abs(126.0, 666.0, 250.0, 46.0) }, ChildOf(root))).with_children(|p| {
        p.spawn((tinted("", pt(10.0), col::ACCENT), Label::PartyLv, Node { position_type: PositionType::Absolute, left: px(10.0), top: px(6.0), ..default() }));
        p.spawn((text("", pt(10.0)), Label::PartyName, Node { position_type: PositionType::Absolute, left: px(50.0), top: px(6.0), ..default() }));
    });
    bar_at(&mut commands, root, 136.0, 698.0, 230.0, 5.0, hex("6FBF4A"), Fill::PartyHp);

    // Bottom centre: the hotbar. Orb slot, weapon slot (ammo, durability, slot dots), bond skill circle.
    let slot = |c: &mut Commands, x: f32, w: f32| {
        c.spawn((BackgroundColor(col::PANEL), BorderColor::all(col::ACCENT.with_alpha(0.7)), Node { border: UiRect::all(px(2.0)), border_radius: BorderRadius::all(px(12.0)), ..abs(x, 960.0, w, 86.0) }, ChildOf(root)));
    };
    slot(&mut commands, 700.0, 150.0);
    slot(&mut commands, 862.0, 232.0);
    commands.spawn((Img::Orb, abs(712.0, 976.0, 54.0, 54.0), ImageNode::default(), ChildOf(root)));
    spawn_text(&mut commands, "", 22.0, Node { position_type: PositionType::Absolute, left: px(772.0), top: px(984.0), ..default() }, Label::OrbCount);
    spawn_text(&mut commands, "", 9.0, Node { position_type: PositionType::Absolute, left: px(700.0), top: px(936.0), width: px(150.0), justify_content: JustifyContent::Center, ..default() }, Label::OrbName);
    commands.spawn((text("Q", pt(9.0)), key_cap(), Node { position_type: PositionType::Absolute, left: px(686.0), top: px(946.0), ..key_cap_node() }, ChildOf(root)));
    commands.spawn((Img::Weapon, abs(876.0, 970.0, 62.0, 62.0), ImageNode::default(), ChildOf(root)));
    spawn_text(&mut commands, "", 9.0, Node { position_type: PositionType::Absolute, left: px(862.0), top: px(936.0), width: px(232.0), justify_content: JustifyContent::Center, ..default() }, Label::WeaponName);
    spawn_text(&mut commands, "", 16.0, Node { position_type: PositionType::Absolute, left: px(1010.0), top: px(972.0), width: px(76.0), justify_content: JustifyContent::Center, ..default() }, Label::Ammo);
    bar_at(&mut commands, root, 950.0, 1022.0, 130.0, 4.0, hex("E9E1CC"), Fill::Durability);
    spawn_text(&mut commands, "", 9.0, Node { position_type: PositionType::Absolute, left: px(944.0), top: px(1030.0), width: px(142.0), justify_content: JustifyContent::Center, ..default() }, Label::Slots);
    commands.spawn((text("", pt(13.0)), Label::ArrowList, Node { position_type: PositionType::Absolute, left: px(862.0), top: px(700.0), width: px(320.0), padding: UiRect::all(px(10.0)), border: UiRect::all(px(1.0)), border_radius: BorderRadius::all(px(10.0)), ..default() }, BackgroundColor(col::PANEL), BorderColor::all(col::ACCENT), Visibility::Hidden, ChildOf(root)));
    commands.spawn((PartnerCircle, Node { border: UiRect::all(px(5.0)), border_radius: BorderRadius::MAX, justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..abs(1108.0, 962.0, 82.0, 82.0) }, BorderColor::all(hex("8CCB5E")), BackgroundColor(col::PANEL), Visibility::Hidden, ChildOf(root))).with_children(|c| {
        c.spawn((tinted("", pt(20.0), col::TEXT), Label::PartnerCd));
    });
    commands.spawn((text("", pt(10.0)), Label::PartnerName, TextLayout::justify(Justify::Left), Node { position_type: PositionType::Absolute, left: px(1200.0), top: px(992.0), width: px(320.0), ..default() }, ChildOf(root)));
    commands.spawn((text("B", pt(10.0)), key_cap(), Node { position_type: PositionType::Absolute, left: px(1200.0), top: px(1030.0), ..key_cap_node() }, ChildOf(root)));
    commands.spawn((tinted("Build", pt(11.0), col::ACCENT), Node { position_type: PositionType::Absolute, left: px(1232.0), top: px(1032.0), ..default() }, ChildOf(root)));
    commands.spawn((text("", pt(11.0)), Label::Hints, TextLayout::justify(Justify::Right), Node { position_type: PositionType::Absolute, right: px(28.0), bottom: px(28.0), ..default() }, ChildOf(root)));

    // Event log (right, under the clock) and the kit readout (F1).
    commands.spawn((text("", pt(10.0)), Label::Log, TextLayout::justify(Justify::Right), Node { position_type: PositionType::Absolute, right: px(20.0), top: px(96.0), width: px(520.0), ..default() }, ChildOf(root)));
    commands.spawn((text("", pt(10.0)), Label::Readout, Node { position_type: PositionType::Absolute, left: px(460.0), top: px(24.0), ..default() }, Visibility::Hidden, ChildOf(root)));
}

/// The compass strip: left edge and width in design pixels (180 degrees across).
const COMPASS_X: f32 = 1480.0;
const COMPASS_W: f32 = 420.0;

fn hide_in_menus(ui: Res<crate::input::UiFocus>, mut q: Query<&mut Visibility, With<HudRoot>>) {
    let want = if ui.0 { Visibility::Hidden } else { Visibility::Inherited };
    for mut v in &mut q {
        if *v != want {
            *v = want;
        }
    }
}

fn key_cap() -> (BackgroundColor, BorderColor, TextLayout) {
    (BackgroundColor(col::PANEL), BorderColor::all(col::ACCENT.with_alpha(0.6)), TextLayout::justify(Justify::Center))
}

fn key_cap_node() -> Node {
    Node { width: px(26.0), height: px(26.0), border: UiRect::all(px(1.0)), border_radius: BorderRadius::all(px(3.0)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }
}

fn bar_at(c: &mut Commands, root: Entity, x: f32, y: f32, w: f32, h: f32, color: Color, f: Fill) {
    c.spawn((abs(x, y, w, h), BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.45)), ChildOf(root))).with_children(|b| {
        b.spawn((Node { width: percent(100.0), height: percent(100.0), ..default() }, BackgroundColor(color), f));
    });
}

fn icon_at(c: &mut Commands, root: Entity, icons: &Icons, id: &str, x: f32, y: f32, s: f32) {
    if let Some(h) = icons.get(id) {
        c.spawn((abs(x, y, s, s), ImageNode::new(h), ChildOf(root)));
    }
}

#[allow(clippy::too_many_arguments)]
fn gauges(db: Res<Db>, progress: Res<Progress>, roster: Res<Creatures>, party: Res<Party>, q: Query<(&Health, &Hunger, &Inventory), With<Player>>, actors: Query<(&CreatureActor, &Health), Without<Player>>, mut fills: Query<(&Fill, &mut Node, &mut BackgroundColor)>) {
    let Ok((hp, hu, inv)) = q.single() else { return };
    let next = ck_core::creature::next_exp(db.rules(), progress.level).unwrap_or(0).max(1) as f32;
    let dur = inv.weapon().and_then(|w| w.durability.map(|d| (d, db.item(&w.item).map_or(d, |i| i.durability.unwrap_or(d).max(1.0)))));
    let sel = roster.0.party().get(party.selected).copied().flatten();
    let creature_hp = sel.and_then(|id| {
        let live = actors.iter().find(|(a, _)| a.id == id).map(|(_, h)| h.hp / h.max);
        live.or_else(|| roster.0.get(id).map(|p| p.hp / p.stats(db.rules(), &db.creatures[&p.species]).max_hp.max(1.0)))
    });
    for (f, mut n, mut bg) in &mut fills {
        let frac = match f {
            Fill::Hp => hp.hp / hp.max,
            Fill::Hunger => hu.value / hu.max,
            Fill::Exp => progress.exp as f32 / next,
            Fill::Durability => dur.map_or(0.0, |(d, m)| d / m),
            Fill::PartyHp => creature_hp.unwrap_or(0.0),
        };
        n.width = percent(frac.clamp(0.0, 1.0) * 100.0);
        if *f == Fill::Hp {
            bg.0 = if frac < 0.25 { hex("E2553F") } else { hex("6FBF4A") };
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn labels(
    db: Res<Db>,
    progress: Res<Progress>,
    log: Res<GameLog>,
    clock: Option<Res<WorldClock>>,
    roster: Res<Creatures>,
    party: Res<Party>,
    partner: Res<PartnerState>,
    inspector: Res<crate::inspector::Inspector>,
    rig: Res<CameraRig>,
    q: Query<(&Health, &Inventory, &Mover, &Locomotion), With<Player>>,
    mut texts: Query<(&Label, &mut Text, &mut Visibility)>,
    picker: Res<crate::inventory::ArrowPicker>,
) {
    let Ok((hp, inv, m, l)) = q.single() else { return };
    let item_name = |id: &str| db.item(id).map_or(id.to_owned(), |i| i.name.clone());
    let w = inv.weapon();
    let bow = w.and_then(|s| db.item(&s.item)).is_some_and(|i| i.sub == "Bow");
    let sel = roster.0.party().get(party.selected).copied().flatten().and_then(|id| roster.0.get(id));
    let next = ck_core::creature::next_exp(db.rules(), progress.level).unwrap_or(0);
    for (lab, mut t, mut vis) in &mut texts {
        let s = match lab {
            Label::HpNum => format!("{:.0}/{:.0}", hp.hp, hp.max),
            Label::Level => format!("{}", progress.level),
            Label::Exp => format!("EXP {}/{}  Tech pts {}", progress.exp, next, progress.tech_points),
            Label::Clock => clock.as_ref().map_or(String::new(), |c| {
                let h = c.hour();
                format!("{:02}:{:02} {}", h as u32, (h.fract() * 60.0) as u32, if c.is_night() { "Night" } else { "Day" })
            }),
            Label::OrbName => item_name(&inv.orb),
            Label::OrbCount => format!("{}", inv.count(&inv.orb)),
            Label::WeaponName => w.map_or("Fists".into(), |s| if bow { format!("{}  ({})", item_name(&s.item), item_name(&inv.ammo)) } else { item_name(&s.item) }),
            Label::Ammo => if bow { format!("{}\n{}", inv.loaded, inv.count(&inv.ammo)) } else { "-".into() },
            Label::Slots => (0..crate::inventory::WEAPON_SLOTS).map(|i| if i == inv.selected { "[#]" } else { " - " }).collect::<Vec<_>>().join(""),
            Label::PartyName => sel.map_or("(no creature)".into(), |p| p.name(&db.creatures[&p.species]).to_owned()),
            Label::PartyLv => sel.map_or(String::new(), |p| format!("Lv {}", p.level)),
            Label::PartnerName => match sel {
                Some(p) if party.active_id == Some(p.id) => {
                    let name = db.creatures[&p.species].bond.as_ref().and_then(|b| db.bonds.get(b)).map_or("Bond Skill", |b| b.name.as_str());
                    if partner.remaining > 0.0 { format!("{name}  {:.0}s", partner.remaining) } else { format!("{name}  [F]") }
                }
                _ => String::new(),
            },
            Label::PartnerCd => if partner.cooldown > 0.0 { format!("{:.0}", partner.cooldown) } else { String::new() },
            Label::ArrowList => {
                *vis = if picker.open && bow { Visibility::Inherited } else { Visibility::Hidden };
                let mut rows = vec!["Arrows (wheel, release R)".to_owned()];
                for (k, a) in crate::inventory::ARROWS.iter().enumerate() {
                    let mark = if k == picker.sel { ">" } else { " " };
                    let using = if *a == inv.ammo { "  *" } else { "" };
                    rows.push(format!("{mark} {}  x{}{using}", item_name(a), inv.count(a)));
                }
                rows.join("\n")
            }
            Label::Readout => {
                *vis = if inspector.on { Visibility::Inherited } else { Visibility::Hidden };
                format!("mode {} | grounded {} | {:.2} m/s | vy {:.2} | pos {:.1} {:.1} {:.1} | cam {} arm {:.2}", l.mode.name(), m.grounded, m.vel.with_y(0.0).length(), m.vel.y, m.feet.x, m.feet.y, m.feet.z, rig.profile, rig.arm)
            }
            Label::Log => {
                if !log.is_changed() {
                    continue;
                }
                log.lines.iter().rev().take(8).rev().map(|(_, s)| s.clone()).collect::<Vec<_>>().join("\n")
            }
            Label::Hints => {
                let mut rows = Vec::new();
                if bow {
                    rows.push("Reload  [R]");
                    rows.push("Arrow type  [hold R]");
                }
                rows.extend(["Command Creature  [4]", if party.active_id.is_some() { "Recall Creature  [E]" } else { "Summon Creature  [E]" }, "Throw Capture Orb  [Q]"]);
                rows.join("\n")
            }
        };
        if t.0 != s {
            t.0 = s;
        }
    }
}

fn images(db: Res<Db>, icons: Res<Icons>, q: Query<&Inventory, With<Player>>, mut imgs: Query<(&Img, &mut ImageNode, &mut Visibility)>, roster: Res<Creatures>, party: Res<Party>) {
    let Ok(inv) = q.single() else { return };
    let ids = roster.0.party();
    for (i, mut node, mut vis) in &mut imgs {
        let h = match i {
            Img::Orb => icons.item(&db, &inv.orb),
            Img::Weapon => inv.weapon().and_then(|w| icons.item(&db, &w.item)),
            Img::Party(k) => {
                let n = ids.len() as i32;
                let slot = (party.selected as i32 + k).rem_euclid(n.max(1)) as usize;
                ids.get(slot).copied().flatten().and_then(|id| roster.0.get(id)).and_then(|p| icons.get(&format!("creature_{}", p.species)))
            }
        };
        *vis = if h.is_some() { Visibility::Inherited } else { Visibility::Hidden };
        if let Some(h) = h
            && node.image != h
        {
            node.image = h;
        }
    }
}

fn party_carousel(party: Res<Party>, mut rings: Query<(&PartyRing, &mut BackgroundColor, &mut BorderColor)>) {
    for (r, mut bg, mut bc) in &mut rings {
        let out = r.0 == 0 && party.active_id.is_some();
        bg.0 = if out { col::ACCENT.with_alpha(0.3) } else { col::PANEL };
        *bc = BorderColor::all(if r.0 == 0 { col::ACCENT } else { col::ACCENT.with_alpha(0.5) });
    }
}

fn partner_circle(party: Res<Party>, partner: Res<PartnerState>, mut q: Query<(&mut Visibility, &mut BorderColor), With<PartnerCircle>>) {
    for (mut v, mut b) in &mut q {
        *v = if party.active_id.is_some() { Visibility::Inherited } else { Visibility::Hidden };
        *b = BorderColor::all(if partner.cooldown > 0.0 { hex("8CCB5E60") } else { hex("8CCB5E") });
    }
}

fn stamina_arc(q: Query<&Stamina, With<Player>>, mut arc: Query<&mut Visibility, With<StaminaArc>>, mut dots: Query<(&StaminaDot, &mut BackgroundColor)>) {
    let Ok(s) = q.single() else { return };
    let frac = (s.sp / s.max).clamp(0.0, 1.0);
    for mut v in &mut arc {
        *v = Visibility::Inherited;
    }
    let lit = (frac * STAMINA_DOTS as f32).ceil() as usize;
    let on = if s.exhausted { hex("E2553F") } else if frac < 0.3 { hex("E08A3C") } else { hex("F2D27A") };
    for (d, mut bg) in &mut dots {
        bg.0 = if d.0 < lit { on } else { Color::srgba(0.0, 0.0, 0.0, 0.25) };
    }
}

/// Ticks slide with the camera yaw: COMPASS_W spans 180 degrees. The Camp Core shows as "camp".
fn compass(rig: Res<CameraRig>, player: Query<&Mover, With<Player>>, bases: Query<&GlobalTransform, With<crate::building::BaseCamp>>, mut ticks: Query<(&CompassTick, &mut Node, &mut Visibility), Without<CompassBase>>, mut base: Query<(&mut Node, &mut Visibility, &mut Text), With<CompassBase>>) {
    // Camera yaw 0 looks along -Z; call -Z north and +X east (heading grows clockwise seen from above).
    let heading = (-rig.yaw).to_degrees().rem_euclid(360.0);
    let place = |deg: f32| {
        let d = (deg - heading + 540.0).rem_euclid(360.0) - 180.0;
        (d.abs() < 86.0).then_some(COMPASS_W / 2.0 + d * COMPASS_W / 180.0)
    };
    for (t, mut n, mut v) in &mut ticks {
        match place(t.0 as f32) {
            Some(x) => {
                n.left = px(x - 6.0);
                *v = Visibility::Inherited;
            }
            None => *v = Visibility::Hidden,
        }
    }
    let (Ok(m), Ok((mut n, mut v, mut txt))) = (player.single(), base.single_mut()) else { return };
    match bases.iter().next() {
        Some(b) => {
            let d = b.translation() - m.feet;
            let deg = d.x.atan2(-d.z).to_degrees().rem_euclid(360.0);
            match place(deg) {
                Some(x) => {
                    n.left = px(x - 20.0);
                    *v = Visibility::Inherited;
                    txt.0 = format!("camp {:.0} m", d.with_y(0.0).length());
                }
                None => *v = Visibility::Hidden,
            }
        }
        None => *v = Visibility::Hidden,
    }
}
