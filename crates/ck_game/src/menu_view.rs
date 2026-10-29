//! Menu screens, laid out in 1920x1080 design pixels: the main-menu tab bar (Inventory, Party, Technology, Build), the inventory with equipment and stats,
//! the technology grid, the build window with
//! category tabs, the crafting station (recipe grid, item card, materials, quantity, Start), the chest (two
//! windows) and the Camp Core (party column, box grid, camp workers, creature detail). Every button has a tooltip.
//! Actions and state live in menus.rs; this file only lays out what it reads from the world.

use bevy::prelude::*;
use bevy::text::FontSize;
use ck_core::inventory::Stack;
use ck_core::creature::{PARTY_SIZE, Place};

use crate::building::{Building, Storage};
use crate::crafting::{Station, station_for};
use crate::data::Db;
use crate::inventory::{Inventory, WEAPON_SLOTS};
use crate::menus::{Action, Menu, MenuButton, MenuState};
use crate::creatures::Creatures;
use crate::player::{Health, Hunger, Player, Stamina};
use crate::progress::Progress;
use crate::ui::{Hoverable, Icons, Scroll, ScrollMemory, Tooltip};

type C<'a> = ChildSpawnerCommands<'a>;

fn hex(s: &str) -> Color {
    Srgba::hex(s).map(Color::from).unwrap_or(Color::WHITE)
}

/// Text sized in points (px = pt x 4/3).
fn t(s: impl Into<String>, pt: f32, color: Color) -> (Text, TextFont, TextColor) {
    (Text::new(s), TextFont { font_size: FontSize::Px(pt * 4.0 / 3.0), ..default() }, TextColor(color))
}

fn abs(x: f32, y: f32, w: f32, h: f32) -> Node {
    Node { position_type: PositionType::Absolute, left: px(x), top: px(y), width: px(w), height: px(h), ..default() }
}

fn white(a: f32) -> Color {
    Color::srgba(1.0, 1.0, 1.0, a)
}

const TEXT: &str = "FFF6E2";
const WINDOW: &str = "11190EE6";
const HEADER: &str = "364231";
const ACCENT: &str = "FFC260";

/// Everything a screen needs, read from the world before spawning.
struct Ctx {
    db: Db,
    icons: Icons,
    inv: Inventory,
    progress: Progress,
    tab: usize,
    scroll: std::collections::HashMap<u8, f32>,
    /// Free crafting: missing materials don't block.
    free: bool,
}

impl Ctx {
    fn item_name(&self, id: &str) -> String {
        self.db.item(id).map_or(id.to_owned(), |i| i.name.clone())
    }
    fn rarity(&self, id: &str) -> u32 {
        self.db.item(id).map_or(0, |i| i.rarity)
    }
    fn item_tip(&self, id: &str, extra: &str) -> Tooltip {
        let Some(i) = self.db.item(id) else { return Tooltip { title: id.to_owned(), body: extra.to_owned() } };
        let mut body = Vec::new();
        body.push(format!("{} {}", i.kind, if i.sub.is_empty() { String::new() } else { format!("/ {}", i.sub) }));
        if !i.desc.is_empty() {
            body.push(wrap(&crate::ui::rich(&self.db, &i.desc), 60));
        }
        let mut stats = vec![format!("Weight {:.1}", i.weight)];
        if i.attack > 0.0 {
            stats.push(format!("Attack {:.0}", i.attack));
        }
        if let Some(d) = i.durability {
            stats.push(format!("Durability {d:.0}"));
        }
        if i.food > 0.0 {
            stats.push(format!("Hunger +{:.0}", i.food));
        }
        body.push(stats.join("   "));
        if !extra.is_empty() {
            body.push(extra.to_owned());
        }
        Tooltip { title: i.name.clone(), body: body.join("\n") }
    }
    fn scroll(&self, key: u8) -> ScrollPosition {
        ScrollPosition(Vec2::new(0.0, self.scroll.get(&key).copied().unwrap_or(0.0)))
    }
}

fn wrap(s: &str, width: usize) -> String {
    let mut out = String::new();
    let mut line = 0;
    for w in s.split_whitespace() {
        if line + w.len() > width && line > 0 {
            out.push('\n');
            line = 0;
        } else if line > 0 {
            out.push(' ');
            line += 1;
        }
        out.push_str(w);
        line += w.len();
    }
    out
}

fn rarity_color(r: u32) -> Option<(Color, Color)> {
    match r {
        1 => Some((hex("54BC3F26"), hex("396C30"))),
        2 => Some((hex("FFAC264D"), hex("BF811D"))),
        3 => Some((hex("CB4FFF4D"), hex("A740D3"))),
        4.. => Some((hex("FFCA004D"), hex("EDBC00"))),
        _ => None,
    }
}

/// A dark window with a faint frame.
fn window(c: &mut C, x: f32, y: f32, w: f32, h: f32, f: impl FnOnce(&mut C)) {
    c.spawn((Node { border: UiRect::all(px(1.0)), ..abs(x, y, w, h) }, BackgroundColor(hex(WINDOW)), BorderColor::all(white(0.12)))).with_children(|w| {
        // Corner dots (12x4).
        for (l, tp) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
            w.spawn((Node { position_type: PositionType::Absolute, left: if l == 0.0 { px(-1.0) } else { auto() }, right: if l == 1.0 { px(-1.0) } else { auto() }, top: if tp == 0.0 { px(-1.0) } else { auto() }, bottom: if tp == 1.0 { px(-1.0) } else { auto() }, width: px(12.0), height: px(4.0), ..default() }, BackgroundColor(hex("F9F5F0"))));
        }
        f(w);
    });
}

/// A 32-px header bar (local coordinates inside a window).
fn header(c: &mut C, x: f32, y: f32, w: f32, title: impl Into<String>) {
    c.spawn((Node { border: UiRect::all(px(1.0)), align_items: AlignItems::Center, padding: UiRect::left(px(12.0)), ..abs(x, y, w, 32.0) }, BackgroundColor(hex(HEADER)), BorderColor::all(white(0.5)))).with_children(|h| {
        h.spawn(t(title, 13.0, hex(TEXT)));
    });
}

/// A section label: 3x18 accent bar + small blue text.
fn section(c: &mut C, x: f32, y: f32, label: &str) {
    c.spawn((abs(x, y + 4.0, 3.0, 18.0), BackgroundColor(hex(ACCENT))));
    c.spawn((t(label, 10.0, hex(ACCENT)), Node { position_type: PositionType::Absolute, left: px(x + 13.0), top: px(y + 2.0), ..default() }));
}

fn button(c: &mut C, node: Node, label: impl Into<String>, pt: f32, act: Option<Action>, tip: Option<Tooltip>) {
    let base = if act.is_some() { hex("FFF6E21A") } else { hex("11190E80") };
    let mut b = c.spawn((Node { border: UiRect::all(px(1.0)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..node }, BackgroundColor(base), BorderColor::all(white(0.3))));
    if let Some(a) = act {
        b.insert((Button, MenuButton(a), Hoverable { base, hover: hex("FFAC2659"), edge: white(0.3) }));
    }
    if let Some(tp) = tip {
        b.insert((Interaction::default(), tp));
    }
    let col = if base == hex("11190E80") { white(0.4) } else { hex(TEXT) };
    b.with_children(|l| {
        l.spawn((t(label, pt, col), TextLayout::justify(Justify::Center)));
    });
}

fn bar(c: &mut C, node: Node, frac: f32, color: Color, label: Option<String>) {
    c.spawn((node, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.45)))).with_children(|b| {
        b.spawn((Node { width: percent(frac.clamp(0.0, 1.0) * 100.0), height: percent(100.0), ..default() }, BackgroundColor(color)));
        if let Some(l) = label {
            b.spawn((t(l, 10.0, Color::WHITE), Node { position_type: PositionType::Absolute, left: px(8.0), top: px(1.0), ..default() }));
        }
    });
}

/// The 76-px item slot: base, rarity wash + bottom line, 1 px frame, icon, count, durability.
fn slot(c: &mut C, ctx: &Ctx, node: Node, size: f32, stack: Option<&Stack>, act: Option<Action>, hint: &str, locked: bool) {
    let rar = stack.and_then(|s| rarity_color(ctx.rarity(&s.item)));
    let base = if locked {
        hex("11180DB3")
    } else if stack.is_none() {
        hex("29302666")
    } else {
        rar.map_or(Color::srgba(0.0, 0.0, 0.0, 0.3), |r| r.0)
    };
    let mut e = c.spawn((Node { width: px(size), height: px(size), border: UiRect::all(px(1.0)), ..node }, BackgroundColor(base), BorderColor::all(white(0.1))));
    if let Some(a) = act {
        e.insert((Button, MenuButton(a), Hoverable { base, hover: hex("FFAC2640"), edge: white(0.1) }));
    }
    if let Some(s) = stack {
        e.insert((Interaction::default(), ctx.item_tip(&s.item, hint)));
    }
    e.with_children(|sl| {
        if let Some((_, line)) = rar {
            sl.spawn((Node { position_type: PositionType::Absolute, left: px(0.0), right: px(0.0), bottom: px(0.0), height: px(3.0), ..default() }, BackgroundColor(line)));
        }
        let Some(s) = stack else {
            if locked {
                sl.spawn((t("-", 12.0, white(0.25)), Node { margin: UiRect::AUTO, ..default() }));
            }
            return;
        };
        match ctx.icons.item(&ctx.db, &s.item) {
            Some(h) => {
                sl.spawn((Node { position_type: PositionType::Absolute, left: px(3.0), top: px(3.0), width: px(size - 8.0), height: px(size - 8.0), ..default() }, ImageNode::new(h)));
            }
            None => {
                sl.spawn((t(ctx.item_name(&s.item), 8.0, hex(TEXT)), Node { position_type: PositionType::Absolute, left: px(4.0), top: px(4.0), width: px(size - 8.0), ..default() }));
            }
        }
        if s.count > 1 {
            sl.spawn((t(s.count.to_string(), 11.0, hex("F7F1ED")), TextShadow { offset: Vec2::splat(1.0), color: Color::srgba(0.0, 0.0, 0.0, 0.8) }, Node { position_type: PositionType::Absolute, right: px(4.0), bottom: px(2.0), ..default() }));
        }
        if let (Some(d), Some(max)) = (s.durability, ctx.db.item(&s.item).and_then(|i| i.durability)) {
            sl.spawn((Node { position_type: PositionType::Absolute, left: px(2.0), right: px(2.0), bottom: px(4.0), height: px(3.0), ..default() }, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)))).with_children(|b| {
                b.spawn((Node { width: percent((d / max.max(1.0)).clamp(0.0, 1.0) * 100.0), height: percent(100.0), ..default() }, BackgroundColor(hex("F8EFDB"))));
            });
        }
    });
}

/// A square tile with an icon (or the name when there is none), used by build / tech / recipes.
#[allow(clippy::too_many_arguments)]
fn tile(c: &mut C, size: f32, icon: Option<Handle<Image>>, name: &str, act: Option<Action>, tip: Tooltip, dim: bool, badge: Option<(String, Color)>, selected: bool) {
    let base = if selected { hex("FFAC2633") } else { hex("11190E4D") };
    let edge = if selected { hex("FFAC26") } else { white(0.15) };
    let mut e = c.spawn((Node { width: px(size), height: px(size), border: UiRect::all(px(if selected { 2.0 } else { 1.0 })), ..default() }, BackgroundColor(base), BorderColor::all(edge), Interaction::default(), tip));
    if let Some(a) = act {
        e.insert((Button, MenuButton(a), Hoverable { base, hover: hex("FDAE2F40"), edge }));
    }
    let has_icon = icon.is_some();
    e.with_children(|tl| {
        match icon {
            Some(h) => {
                let s = size * 0.78;
                tl.spawn((Node { position_type: PositionType::Absolute, left: px((size - s) / 2.0 - 1.0), top: px(3.0), width: px(s), height: px(s), ..default() }, ImageNode::new(h)));
            }
            None => {
                tl.spawn((t(name, 9.0, hex(TEXT)), Node { position_type: PositionType::Absolute, left: px(5.0), top: px(6.0), width: px(size - 10.0), ..default() }));
            }
        }
        if has_icon {
            tl.spawn((Node { position_type: PositionType::Absolute, left: px(0.0), right: px(0.0), bottom: px(0.0), padding: UiRect::axes(px(3.0), px(1.0)), justify_content: JustifyContent::Center, ..default() }, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)))).with_children(|n| {
                n.spawn((t(name, 7.5, white(0.9)), TextLayout::new(Justify::Center, LineBreak::WordBoundary)));
            });
        }
        if dim {
            tl.spawn((Node { position_type: PositionType::Absolute, left: px(0.0), top: px(0.0), width: percent(100.0), height: percent(100.0), ..default() }, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55))));
        }
        if let Some((b, col)) = badge {
            tl.spawn((Node { position_type: PositionType::Absolute, right: px(2.0), top: px(2.0), padding: UiRect::axes(px(4.0), px(0.0)), ..default() }, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)))).with_children(|n| {
                n.spawn(t(b, 9.0, col));
            });
        }
    });
}

pub fn rebuild(world: &mut World) {
    let dt = world.resource::<Time>().delta_secs();
    let (menu, dirty, root) = {
        let mut st = world.resource_mut::<MenuState>();
        st.refresh -= dt;
        // Live screens (progress bars, HP) refresh; the rest rebuild on change only.
        let live = matches!(st.menu, Menu::Station(_) | Menu::Storage(_) | Menu::CampCore(_) | Menu::Creature(_) | Menu::WorkBoard(_));
        let due = live && st.refresh <= 0.0;
        if due {
            st.refresh = 0.5;
        }
        (st.menu, st.dirty || due, st.root)
    };
    if !dirty {
        return;
    }
    world.resource_mut::<MenuState>().dirty = false;
    if let Some(r) = root {
        if world.get_entity(r).is_ok() {
            world.despawn(r);
        }
        world.resource_mut::<MenuState>().root = None;
    }
    if menu == Menu::None {
        return;
    }
    let Some(pe) = world.query_filtered::<Entity, With<Player>>().iter(world).next() else { return };
    let st = world.resource::<MenuState>();
    let (tab, msg) = (st.tab, st.msg.clone());
    let ctx = Ctx {
        db: world.resource::<Db>().clone(),
        icons: world.resource::<Icons>().clone(),
        inv: world.get::<Inventory>(pe).cloned().unwrap_or_else(Inventory::new),
        progress: world.resource::<Progress>().clone(),
        tab,
        scroll: world.resource::<ScrollMemory>().0.clone(),
        free: world.resource::<crate::crafting::FreeCraft>().0,
    };
    let screen: Box<dyn FnOnce(&mut C)> = match menu {
        Menu::Inventory => inventory(world, pe, ctx),
        Menu::Tech => tech(ctx),
        Menu::Build => build(ctx),
        Menu::Station(e) => match station(world, e, ctx) {
            Some(f) => f,
            None => {
                world.resource_mut::<MenuState>().menu = Menu::None;
                return;
            }
        },
        Menu::Storage(e) => match storage(world, e, ctx) {
            Some(f) => f,
            None => {
                world.resource_mut::<MenuState>().menu = Menu::None;
                return;
            }
        },
        Menu::CampCore(_) => camp_core(world, ctx),
        Menu::Repair(_) => repair_screen(ctx),
        Menu::Settings => settings_screen(world),
        Menu::WorkBoard(_) => work_board(world, ctx),
        Menu::Creature(id) => party(world, ctx, id),
        Menu::None => return,
    };
    let main_tab = match menu {
        Menu::Inventory => Some(0),
        Menu::Creature(_) => Some(1),
        Menu::Tech => Some(2),
        Menu::Build => Some(3),
        _ => None,
    };
    let first_party = world.resource::<Creatures>().0.party().iter().flatten().next().copied().unwrap_or(0);
    let mut commands = world.commands();
    let root = commands
        .spawn((Node { position_type: PositionType::Absolute, width: percent(100.0), height: percent(100.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }, BackgroundColor(hex("11190EB8")), GlobalZIndex(10)))
        .with_children(|r| {
            r.spawn(Node { width: px(1920.0), height: px(1080.0), flex_shrink: 0.0, ..default() }).with_children(|c| {
                screen(c);
                if let Some(sel) = main_tab {
                    // Tab bar at (84,48).
                    for (i, (label, key, m)) in [("Inventory", "Tab", Menu::Inventory), ("Party", "", Menu::Creature(first_party)), ("Technology", "T", Menu::Tech), ("Build", "B", Menu::Build)].into_iter().enumerate() {
                        let on = i == sel;
                        let base = if on { hex("C6BEAC") } else { hex("11190E99") };
                        c.spawn((Button, MenuButton(Action::Open(m)), Hoverable { base, hover: if on { base } else { hex("FFAC2640") }, edge: Color::NONE }, Node { border: UiRect::bottom(px(3.0)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, column_gap: px(8.0), ..abs(84.0 + i as f32 * 158.0, 48.0, 150.0, 40.0) }, BackgroundColor(base), BorderColor::all(if on { hex(ACCENT) } else { Color::NONE }))).with_children(|b| {
                            b.spawn(t(label, 14.0, if on { hex("11190E") } else { hex("FFFBF280") }));
                            if !key.is_empty() {
                                b.spawn(t(key, 9.0, if on { hex("11190EA0") } else { hex("FFFBF260") }));
                            }
                        });
                    }
                }
                button(c, abs(1822.0, 40.0, 60.0, 40.0), "X  Esc", 10.0, Some(Action::Close), None);
                if !msg.is_empty() {
                    let bad = msg.starts_with("rejected");
                    c.spawn((Node { position_type: PositionType::Absolute, left: px(460.0), top: px(1030.0), width: px(1000.0), height: px(36.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, border: UiRect::top(px(2.0)), ..default() }, BackgroundColor(if bad { hex("300000E6") } else { hex("11190EE6") }), BorderColor::all(if bad { hex("FF3F3F") } else { hex(ACCENT) }))).with_children(|m| {
                        m.spawn(t(msg, 12.0, if bad { hex("FF7F7F") } else { hex(TEXT) }));
                    });
                }
            });
        })
        .id();
    world.flush();
    world.resource_mut::<MenuState>().root = Some(root);
}

fn inventory(world: &mut World, pe: Entity, ctx: Ctx) -> Box<dyn FnOnce(&mut C)> {
    let hp = world.get::<Health>(pe).map_or((0.0, 1.0), |h| (h.hp, h.max));
    let hunger = world.get::<Hunger>(pe).map_or((0.0, 1.0), |h| (h.value, h.max));
    let sp = world.get::<Stamina>(pe).map_or(0.0, |s| s.max);
    let (attack, defense) = world.get::<crate::combat::Combatant>(pe).map_or((0.0, 0.0), |c| (c.melee_attack, c.defense));
    let work = crate::crafting::player_work_speed(&ctx.db, &ctx.progress);
    Box::new(move |c: &mut C| {
        let inv = &ctx.inv;
        let p = &ctx.progress;
        // Left: item list (148,128)-(736,964).
        window(c, 148.0, 128.0, 588.0, 836.0, |w| {
            w.spawn((Node { border: UiRect::all(px(1.0)), ..abs(0.0, 0.0, 588.0, 32.0) }, BackgroundColor(hex("364231")), BorderColor::all(white(0.5))));
            w.spawn((Node { justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..abs(4.0, 2.0, 230.0, 28.0) }, BackgroundColor(hex("C6BEAC")))).with_children(|tb| {
                tb.spawn(t("Inventory", 13.0, hex("11190E")));
            });
            button(w, abs(466.0, 2.0, 117.0, 28.0), "Sort", 11.0, Some(Action::Sort), Some(Tooltip { title: "Sort".into(), body: "Stack and order the bag by item type.".into() }));
            w.spawn((Node { display: Display::Grid, grid_template_columns: RepeatedGridTrack::px(6, 76.0), column_gap: px(10.0), row_gap: px(10.0), ..abs(41.0, 48.0, 506.0, 640.0) },)).with_children(|g| {
                for (i, s) in inv.bag.slots.iter().enumerate() {
                    let hint = s.as_ref().and_then(|s| ctx.db.item(&s.item)).map_or("", |it| if it.kind == "Food" { "Click: eat" } else if matches!(it.kind.as_str(), "Weapon" | "Armor" | "Glider" | "Orb") { "Click: equip" } else { "" });
                    slot(g, &ctx, Node::default(), 76.0, s.as_ref(), s.as_ref().map(|_| Action::UseBag(i)), hint, false);
                }
            });
            w.spawn((t("Click food to eat it, gear to equip it. Hover any slot for details.", 10.0, white(0.55)), Node { position_type: PositionType::Absolute, left: px(41.0), top: px(706.0), ..default() }));
            // Weight bar.
            let (wt, max) = (inv.weight(&ctx.db).max(0.0), inv.max_weight(&ctx.db));
            w.spawn((abs(0.0, 795.0, 588.0, 32.0), BackgroundColor(hex("FFEEC633")))).with_children(|r| {
                if let Some(h) = ctx.icons.get("stat_weight") {
                    r.spawn((abs(4.0, 2.0, 28.0, 28.0), ImageNode::new(h)));
                }
                r.spawn((t("Weight", 12.0, hex(TEXT)), Node { position_type: PositionType::Absolute, left: px(36.0), top: px(6.0), ..default() }));
                r.spawn((Node { border: UiRect::all(px(1.0)), padding: UiRect::all(px(2.0)), ..abs(128.0, 9.0, 272.0, 14.0) }, BorderColor::all(hex(TEXT)), BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)))).with_children(|b| {
                    b.spawn((Node { width: percent((wt / max.max(1.0)).clamp(0.0, 1.0) * 100.0), height: percent(100.0), ..default() }, BackgroundColor(if wt > max { hex("FF3F3F") } else { hex(TEXT) })));
                });
                r.spawn((t(format!("{wt:.1} / {max:.0}"), 12.0, if wt > max { hex("FF3F3F") } else { hex(TEXT) }), Node { position_type: PositionType::Absolute, left: px(420.0), top: px(6.0), ..default() }));
            });
        });
        // Right: character, equipment and stats (772,128)-(1768,924).
        window(c, 772.0, 128.0, 996.0, 796.0, |w| {
            header(w, 0.0, 0.0, 996.0, format!("Player    Lv {}", p.level));
            let at = |x: f32, y: f32| abs(x - 772.0, y - 128.0, 76.0, 76.0);
            section(w, 31.0, 64.0, "Weapon");
            for i in 0..WEAPON_SLOTS {
                let s = inv.weapons[i].as_ref();
                let y = 220.0 + i as f32 * 88.0;
                if i == inv.selected {
                    w.spawn((abs(803.0 - 772.0 - 13.0, y - 128.0, 5.0, 76.0), BackgroundColor(hex("FFAC26"))));
                }
                slot(w, &ctx, at(803.0, y), 76.0, s, s.map(|_| Action::Unequip(i)), &format!("Weapon slot {} (keys 1-4 / wheel). Click: unequip", i + 1), false);
            }
            section(w, 43.0, 470.0, "Accessory");
            for (x, y) in [(815.0, 626.0), (903.0, 626.0), (815.0, 714.0), (903.0, 714.0)] {
                slot(w, &ctx, at(x, y), 76.0, None, None, "", true);
            }
            for (y, label) in [(192.0, "Head"), (326.0, "Body"), (454.0, "Shield"), (592.0, "Glider"), (728.0, "Orb Module")] {
                section(w, 1279.0 - 772.0, y - 128.0, label);
            }
            slot(w, &ctx, at(1279.0, 220.0), 76.0, inv.head.as_ref(), inv.head.as_ref().map(|_| Action::UnequipHead), "Head armor. Click: unequip", false);
            slot(w, &ctx, at(1279.0, 354.0), 76.0, inv.armor.as_ref(), inv.armor.as_ref().map(|_| Action::UnequipArmor), "Body armor. Click: unequip", false);
            slot(w, &ctx, at(1279.0, 482.0), 76.0, None, None, "", true);
            slot(w, &ctx, at(1279.0, 620.0), 76.0, inv.glider.as_ref(), inv.glider.as_ref().map(|_| Action::UnequipGlider), "Glider: hold Space in the air. Click: unequip", false);
            let orb = Stack { item: inv.orb.clone(), count: inv.count(&inv.orb), durability: None };
            slot(w, &ctx, at(1279.0, 756.0), 76.0, Some(&orb), None, "The orb Q throws. 2: next orb type.", false);
            section(w, 43.0, 688.0, "Food");
            for i in 0..5 {
                slot(w, &ctx, at(815.0 + i as f32 * 88.0, 844.0), 76.0, None, None, "", true);
            }
            // Player preview placeholder (a character render could go here).
            w.spawn((Node { border_radius: BorderRadius::MAX, ..abs(240.0, 150.0, 120.0, 120.0) }, BackgroundColor(white(0.06))));
            w.spawn((Node { border_radius: BorderRadius::all(px(60.0)), ..abs(205.0, 280.0, 190.0, 330.0) }, BackgroundColor(white(0.05))));
            // Status block (1441,184).
            let sx = 1441.0 - 772.0;
            w.spawn((t("LEVEL", 10.0, white(0.6)), Node { position_type: PositionType::Absolute, left: px(sx), top: px(60.0), ..default() }));
            w.spawn((t(p.level.to_string(), 36.0, hex(TEXT)), Node { position_type: PositionType::Absolute, left: px(sx), top: px(72.0), ..default() }));
            let next = ck_core::creature::next_exp(ctx.db.rules(), p.level).unwrap_or(0);
            w.spawn((t(format!("EXP {} / {next}\nTech points {}", p.exp, p.tech_points), 10.0, white(0.7)), Node { position_type: PositionType::Absolute, left: px(sx + 90.0), top: px(80.0), ..default() }));
            bar(w, abs(sx, 128.0, 299.0, 4.0), p.exp as f32 / next.max(1) as f32, hex("FDAE2F"), None);
            bar(w, abs(sx + 4.0, 158.0, 244.0, 22.0), hp.0 / hp.1, hex("26BF73"), Some(format!("HP {:.0} / {:.0}", hp.0, hp.1)));
            bar(w, abs(sx + 4.0, 188.0, 244.0, 22.0), hunger.0 / hunger.1, hex("FF8000"), Some(format!("Hunger {:.0} / {:.0}", hunger.0, hunger.1)));
            // Stats (1441,416).
            w.spawn((abs(sx, 297.0, 307.0, 14.0), BackgroundColor(hex("95918980"))));
            w.spawn((t("Stats", 10.0, Color::WHITE), Node { position_type: PositionType::Absolute, left: px(sx + 8.0), top: px(294.0), ..default() }));
            w.spawn((Node { border: UiRect::left(px(3.0)), justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Center, padding: UiRect::axes(px(8.0), px(0.0)), ..abs(sx + 6.0, 323.0, 295.0, 28.0) }, BorderColor::all(hex("CBBFA480")), BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.3)))).with_children(|r| {
                r.spawn(t("Stat Points", 10.0, hex("EADDBE")));
                r.spawn(t(p.status_points.to_string(), 14.0, if p.status_points > 0 { hex("FFE645") } else { hex("EADDBE") }));
            });
            // (label, value, alloc index for +, icon)
            let rows: [(&str, String, Option<usize>, &str); 6] = [
                ("Health", format!("{:.0}", hp.1), Some(0), "stat_hp"),
                ("Stamina", format!("{sp:.0}"), Some(1), ""),
                ("Attack", format!("{attack:.0}"), Some(2), "stat_attack"),
                ("Defense", format!("{defense:.0}"), None, ""),
                ("Work Speed", format!("{work:.0}"), Some(4), "work_craft"),
                ("Weight", format!("{:.0}", inv.max_weight(&ctx.db)), Some(3), "stat_weight"),
            ];
            for (k, (label, value, alloc, icon)) in rows.into_iter().enumerate() {
                let y = 364.0 + k as f32 * 38.0;
                w.spawn((Node { align_items: AlignItems::Center, padding: UiRect::axes(px(4.0), px(0.0)), column_gap: px(6.0), ..abs(sx + 8.0, y, 250.0, 32.0) }, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.3)))).with_children(|r| {
                    match ctx.icons.get(icon) {
                        Some(h) => {
                            r.spawn((Node { width: px(24.0), height: px(24.0), ..default() }, ImageNode::new(h)));
                        }
                        None => {
                            r.spawn(Node { width: px(24.0), ..default() });
                        }
                    }
                    r.spawn((t(label, 11.0, white(0.75)), Node { flex_grow: 1.0, ..default() }));
                    let n = alloc.map_or(0, |a| p.alloc[a]);
                    r.spawn(t(if n > 0 { format!("{value}  (+{n})") } else { value.clone() }, 13.0, hex(TEXT)));
                });
                if let Some(a) = alloc {
                    let can = p.status_points > 0;
                    button(w, abs(sx + 264.0, y + 1.0, 30.0, 30.0), "+", 14.0, can.then_some(Action::Status(a)), Some(Tooltip { title: format!("{label} +1"), body: if can { "Spend one stat point.".into() } else { "No stat points left (one per level).".into() } }));
                }
            }
        });
    })
}

fn tech(ctx: Ctx) -> Box<dyn FnOnce(&mut C)> {
    Box::new(move |c: &mut C| {
        let p = &ctx.progress;
        window(c, 148.0, 128.0, 1620.0, 836.0, |w| {
            header(w, 0.0, 0.0, 1620.0, format!("Technology        Technology Points  {}        Player Lv {}", p.tech_points, p.level));
            w.spawn((t("Click a tile to unlock it (costs points; needs the player level). Unlocked recipes appear at their station, objects in Build.", 10.0, white(0.6)), Node { position_type: PositionType::Absolute, left: px(16.0), top: px(42.0), ..default() }));
            w.spawn((Scroll(1), ctx.scroll(1), Node { overflow: Overflow::scroll_y(), flex_direction: FlexDirection::Column, row_gap: px(10.0), ..abs(16.0, 70.0, 1588.0, 750.0) })).with_children(|list| {
                let mut levels: Vec<u32> = ctx.db.technologies.iter().map(|t| t.level).filter(|l| *l <= 20).collect();
                levels.sort();
                levels.dedup();
                for lv in levels {
                    list.spawn(Node { flex_direction: FlexDirection::Row, column_gap: px(12.0), flex_shrink: 0.0, ..default() }).with_children(|row| {
                        let reached = p.level >= lv;
                        row.spawn((Node { width: px(64.0), height: px(140.0), border: UiRect::left(px(3.0)), padding: UiRect::left(px(8.0)), flex_direction: FlexDirection::Column, flex_shrink: 0.0, ..default() }, BorderColor::all(if reached { hex(ACCENT) } else { white(0.2) }))).with_children(|l| {
                            l.spawn(t("Lv", 10.0, white(0.6)));
                            l.spawn(t(lv.to_string(), 26.0, if reached { hex(TEXT) } else { white(0.35) }));
                        });
                        row.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: px(8.0), row_gap: px(8.0), width: px(1500.0), ..default() }).with_children(|g| {
                            for tc in ctx.db.technologies.iter().filter(|t| t.level == lv) {
                                let done = p.unlocked.contains(&tc.id);
                                let icon = tc.builds.first().and_then(|b| ctx.icons.build(b)).or_else(|| tc.recipes.first().and_then(|r| ctx.db.recipe(r)).and_then(|r| ctx.icons.item(&ctx.db, &r.product)));
                                let unlocks: Vec<String> = tc.builds.iter().map(|b| ctx.db.build(b).map_or(b.clone(), |x| x.name.clone())).chain(tc.recipes.iter().map(|r| ctx.db.recipe(r).map_or(r.clone(), |x| ctx.item_name(&x.product)))).collect();
                                let state = if done {
                                    "Unlocked".to_owned()
                                } else if ctx.free {
                                    "Free mode: click to unlock".to_owned()
                                } else if !reached {
                                    format!("Needs Player Lv {lv}")
                                } else if p.tech_points < tc.cost {
                                    format!("Needs {} points (you have {})", tc.cost, p.tech_points)
                                } else {
                                    format!("Click to unlock for {} point(s)", tc.cost)
                                };
                                let tip = Tooltip { title: tc.name.clone(), body: format!("{}\nUnlocks: {}\n{state}", wrap(&crate::ui::rich(&ctx.db, &tc.desc), 60), unlocks.join(", ")) };
                                let badge = if done { ("OK".to_owned(), hex("1EFF9E")) } else if ctx.free { ("free".to_owned(), hex("1EFF9E")) } else { (tc.cost.to_string(), if p.tech_points >= tc.cost { hex("FFE645") } else { hex("FF3F3F") }) };
                                let open = reached || ctx.free;
                                tile(g, 112.0, icon, &tc.name, (!done && open).then(|| Action::Unlock(tc.id.clone())), tip, !open, Some(badge), false);
                            }
                        });
                    });
                }
            });
        });
    })
}

/// Build menu tabs: (building `category`, tab label).
const BUILD_TABS: [(&str, &str); 9] = [("Production", "Production"), ("Creatures", "Creatures"), ("Storage", "Storage"), ("Food", "Food"), ("Camp", "Camp"), ("Lighting", "Lighting"), ("Foundation", "Foundations"), ("Defense", "Defenses"), ("Furniture", "Furniture")];

fn build(ctx: Ctx) -> Box<dyn FnOnce(&mut C)> {
    Box::new(move |c: &mut C| {
        let p = &ctx.progress;
        let avail: Vec<_> = ctx.db.builds.iter().filter(|b| p.build_unlocked(&ctx.db, &b.id) || b.id == "camp_core").collect();
        let tab = ctx.tab.min(BUILD_TABS.len() - 1);
        // Category tabs above the window, centred, bottom at y=145.
        for (i, (key, label)) in BUILD_TABS.iter().enumerate() {
            let n = avail.iter().filter(|b| b.category == *key).count();
            let on = i == tab;
            let x = 960.0 - BUILD_TABS.len() as f32 * 58.0 + i as f32 * 116.0;
            let base = if on { hex("11190EF2") } else { hex("11190EBF") };
            c.spawn((Button, MenuButton(Action::Tab(i)), Hoverable { base, hover: hex("FFAC2640"), edge: Color::NONE }, Node { border: UiRect::bottom(px(if on { 4.0 } else { 0.0 })), flex_direction: FlexDirection::Column, align_items: AlignItems::Center, justify_content: JustifyContent::End, padding: UiRect::bottom(px(6.0)), ..abs(x, 100.0, 114.0, 76.0) }, BackgroundColor(base), BorderColor::all(hex("FFAC26")))).with_children(|tb| {
                let icon = avail.iter().find(|b| b.category == *key).and_then(|b| ctx.icons.build(&b.id));
                if let Some(h) = icon {
                    tb.spawn((Node { width: px(40.0), height: px(40.0), margin: UiRect::bottom(px(2.0)), ..default() }, ImageNode::new(h).with_color(white(if on { 1.0 } else { 0.4 }))));
                }
                tb.spawn(t(format!("{label} {n}"), 10.0, white(if on { 1.0 } else { 0.6 })));
            });
        }
        window(c, 325.0, 196.0, 1270.0, 800.0, |w| {
            w.spawn((Node { border: UiRect::left(px(3.0)), align_items: AlignItems::Center, padding: UiRect::left(px(15.0)), ..abs(24.0, 12.0, 1222.0, 36.0) }, BorderColor::all(hex(ACCENT)), BackgroundGradient::from(LinearGradient::to_right(vec![ColorStop::auto(hex("3B552F")), ColorStop::auto(hex("3B552F00"))])))).with_children(|h| {
                h.spawn(t(BUILD_TABS[tab].1, 14.0, hex(TEXT)));
            });
            w.spawn((Scroll(2), ctx.scroll(2), Node { overflow: Overflow::scroll_y(), display: Display::Grid, grid_template_columns: RepeatedGridTrack::px(10, 108.0), grid_auto_rows: vec![GridTrack::px(108.0)], column_gap: px(9.0), row_gap: px(9.0), ..abs(34.0, 64.0, 1200.0, 640.0) })).with_children(|g| {
                for b in avail.iter().filter(|b| b.category == BUILD_TABS[tab].0) {
                    let enough = ctx.free || b.materials.iter().all(|(m, n)| ctx.inv.count(m) >= *n);
                    let mats = b.materials.iter().map(|(m, n)| format!("{}  {} / {n}{}", ctx.item_name(m), ctx.inv.count(m), if ctx.inv.count(m) >= *n { "" } else { "  (missing)" })).collect::<Vec<_>>().join("\n");
                    let suit = b.jobs.iter().map(|j| format!("{} {}", crate::base_tools::suit_name(&ctx.db, &j.job), j.rank)).collect::<Vec<_>>().join(", ");
                    let body = format!("{}\n\nMaterials:\n{mats}\nWork {:.0}{}{}", wrap(&crate::ui::rich(&ctx.db, &b.desc), 60), b.work, if suit.is_empty() { String::new() } else { format!("\nWorked by: {suit}") }, if enough { "\n\nClick to place".to_owned() } else { "\n\nNot enough materials (F2 = free mode)".to_owned() });
                    tile(g, 108.0, ctx.icons.build(&b.id), &b.name, Some(Action::Build(b.id.clone())), Tooltip { title: b.name.clone(), body }, !enough, None, false);
                }
            });
            w.spawn((abs(24.0, 724.0, 1222.0, 2.0), BackgroundColor(hex("F1F0EF4D"))));
            w.spawn((t("Click an object to place it (LMB place, R rotate, RMB / Esc cancel), then hold F at the blueprint or let camp creatures build it.", 11.0, white(0.6)), Node { position_type: PositionType::Absolute, left: px(30.0), top: px(742.0), ..default() }));
            button(w, abs(1036.0, 734.0, 210.0, 40.0), "Dismantle  (X)", 13.0, Some(Action::Dismantle), Some(Tooltip { title: "Dismantle".into(), body: "Aim at a building and hold LMB: it comes apart and every material goes back to the bag (chest contents and station output too). X or Esc leaves the mode.".into() }));
        });
    })
}

/// Recipe tabs (`recipe_tab` picks the one for a product).
const RECIPE_TABS: [&str; 7] = ["All", "Orb", "Weapon", "Armor", "Tool", "Consumable", "Material"];

fn recipe_tab(db: &Db, product: &str) -> usize {
    let Some(i) = db.item(product) else { return 6 };
    let l = product.to_ascii_lowercase();
    if l.starts_with("axe") || l.starts_with("pickaxe") || l.contains("torch") {
        return 4;
    }
    match i.kind.as_str() {
        "Orb" => 1,
        "Weapon" => 2,
        "Armor" | "Glider" => 3,
        "Food" | "Consume" | "Ammo" => 5,
        _ => 6,
    }
}

fn station(world: &mut World, e: Entity, ctx: Ctx) -> Option<Box<dyn FnOnce(&mut C)>> {
    let st = world.get::<Station>(e)?.clone();
    let name = world.get::<Building>(e).map_or("Station".into(), |b| b.name.clone());
    let kind = world.get::<Building>(e).map_or("workbench".into(), |b| b.id.to_ascii_lowercase());
    let workers = world.resource::<crate::base::WorkContributions>().rate(e);
    let (sel, qty) = {
        let ms = world.resource::<MenuState>();
        (ms.sel.clone(), ms.qty.max(1))
    };
    Some(Box::new(move |c: &mut C| {
        let db = &ctx.db;
        let p = &ctx.progress;
        let recipes: Vec<_> = db.recipes.iter().filter(|r| p.recipe_unlocked(db, &r.id) && station_for(db, &r.id) == kind).collect();
        let tab = ctx.tab.min(RECIPE_TABS.len() - 1);
        let shown: Vec<_> = recipes.iter().filter(|r| tab == 0 || recipe_tab(db, &r.product) == tab).collect();
        let sel = sel.as_ref().and_then(|s| recipes.iter().find(|r| &r.id == s)).or(shown.first().copied()).copied();
        window(c, 390.0, 70.0, 1140.0, 940.0, |w| {
            header(w, 16.0, 22.0, 1108.0, format!("{name}    Recipe List"));
            // Category tabs (410,140)-(1510,210).
            for (i, label) in RECIPE_TABS.iter().enumerate() {
                let n = recipes.iter().filter(|r| i == 0 || recipe_tab(db, &r.product) == i).count();
                let on = i == tab;
                let base = if on { hex("FFC26040") } else { hex("11190E80") };
                w.spawn((Button, MenuButton(Action::Tab(i)), Hoverable { base, hover: hex("FFC26030"), edge: Color::NONE }, Node { border: UiRect::bottom(px(3.0)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..abs(20.0 + i as f32 * 157.0, 70.0, 153.0, 56.0) }, BackgroundColor(base), BorderColor::all(if on { hex(ACCENT) } else { white(0.08) }))).with_children(|b| {
                    b.spawn(t(format!("{label} ({n})"), 12.0, white(if on { 1.0 } else { if n > 0 { 0.65 } else { 0.3 } })));
                });
            }
            // Recipe grid, 5 columns of 100 px.
            w.spawn((Scroll(3), ctx.scroll(3), Node { overflow: Overflow::scroll_y(), display: Display::Grid, grid_template_columns: RepeatedGridTrack::px(5, 100.0), grid_auto_rows: vec![GridTrack::px(100.0)], column_gap: px(4.0), row_gap: px(4.0), ..abs(34.0, 148.0, 532.0, 640.0) })).with_children(|g| {
                for r in &shown {
                    let enough = ctx.free || r.materials.iter().all(|(m, n)| ctx.inv.count(m) >= *n);
                    let mats = r.materials.iter().map(|(m, n)| format!("{}  {} / {n}", ctx.item_name(m), ctx.inv.count(m))).collect::<Vec<_>>().join("\n");
                    let tip = ctx.item_tip(&r.product, &format!("Makes {}\n{mats}\nWork {:.0}\nClick to select", r.count, r.work));
                    let is_sel = sel.is_some_and(|s| s.id == r.id);
                    tile(g, 100.0, ctx.icons.item(db, &r.product), &ctx.item_name(&r.product), Some(Action::Select(r.id.clone())), tip, !enough, (r.count > 1).then(|| (format!("x{}", r.count), Color::WHITE)), is_sel);
                }
            });
            if shown.is_empty() {
                w.spawn((t("No usable recipes. Try unlocking recipes from the Technology menu (T).", 12.0, white(0.6)), Node { position_type: PositionType::Absolute, left: px(40.0), top: px(170.0), width: px(500.0), ..default() }));
            }
            // Right: item card, materials, workload, quantity, Start.
            let Some(r) = sel else { return };
            w.spawn((Node { border: UiRect::top(px(3.0)), ..abs(620.0, 148.0, 476.0, 464.0) }, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.2)), BorderColor::all(rarity_color(ctx.rarity(&r.product)).map_or(hex("AA9E82"), |x| x.1)))).with_children(|card| {
                card.spawn((abs(0.0, 0.0, 476.0, 205.0), BackgroundGradient::from(LinearGradient::to_bottom(vec![ColorStop::auto(hex("0F150B")), ColorStop::auto(hex("BF811D33"))]))));
                if let Some(h) = ctx.icons.item(db, &r.product) {
                    card.spawn((abs(174.0, 21.0, 128.0, 128.0), ImageNode::new(h)));
                }
                card.spawn((t(format!("{}  x{}", ctx.item_name(&r.product), r.count), 15.0, Color::WHITE), TextLayout::justify(Justify::Center), Node { position_type: PositionType::Absolute, left: px(0.0), top: px(160.0), width: px(476.0), justify_content: JustifyContent::Center, ..default() }));
                card.spawn((t(format!("Owned {}", ctx.inv.count(&r.product)), 12.0, hex("CCCCCC")), Node { position_type: PositionType::Absolute, right: px(12.0), top: px(8.0), ..default() }));
                if let Some(i) = db.item(&r.product)
                    && !i.desc.is_empty()
                {
                    card.spawn((t(wrap(&crate::ui::rich(db, &i.desc), 70), 9.0, white(0.75)), Node { position_type: PositionType::Absolute, left: px(14.0), top: px(186.0), width: px(448.0), ..default() }));
                }
                for (k, (m, n)) in r.materials.iter().enumerate() {
                    let need = n * qty;
                    let have = ctx.inv.count(m);
                    card.spawn((Node { border: UiRect::all(px(1.0)), align_items: AlignItems::Center, ..abs(0.0, 240.0 + k as f32 * 44.0, 476.0, 40.0) }, BackgroundColor(hex("8D887D4D")), BorderColor::all(white(0.3)), Interaction::default(), ctx.item_tip(m, ""))).with_children(|row| {
                        if let Some(h) = ctx.icons.item(db, m) {
                            row.spawn((Node { width: px(36.0), height: px(36.0), margin: UiRect::left(px(6.0)), ..default() }, ImageNode::new(h)));
                        }
                        row.spawn((t(ctx.item_name(m), 11.0, hex("F9F5F0")), Node { margin: UiRect::left(px(10.0)), flex_grow: 1.0, ..default() }));
                        row.spawn((Node { width: px(110.0), height: percent(100.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }, BackgroundColor(hex("161F11B3")))).with_children(|b| {
                            b.spawn(t(format!("{have} / {need}"), 12.0, if have >= need { Color::WHITE } else { hex("FF3F3F") }));
                        });
                    });
                }
                card.spawn((Node { justify_content: JustifyContent::SpaceBetween, ..abs(15.0, 430.0, 446.0, 28.0) },)).with_children(|row| {
                    row.spawn(t("Work", 12.0, hex("BCAF91")));
                    row.spawn(t(format!("{:.0}", r.work * qty as f32), 14.0, hex("FF9100")));
                });
            });
            // Quantity selector.
            w.spawn((Node { border: UiRect::all(px(1.0)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, column_gap: px(10.0), ..abs(624.0, 640.0, 468.0, 96.0) }, BorderColor::all(hex("FFC567B3")), BackgroundColor(hex("FFAC260D")))).with_children(|q| {
                for (label, d) in [("<<", -10), ("<", -1)] {
                    button(q, Node { width: px(44.0), height: px(44.0), ..default() }, label, 14.0, Some(Action::Qty(d)), None);
                }
                q.spawn((t(format!("{qty}"), 28.0, Color::WHITE), Node { width: px(110.0), justify_content: JustifyContent::Center, ..default() }, TextLayout::justify(Justify::Center)));
                for (label, d) in [(">", 1), (">>", 10)] {
                    button(q, Node { width: px(44.0), height: px(44.0), ..default() }, label, 14.0, Some(Action::Qty(d)), None);
                }
            });
            w.spawn((abs(620.0, 760.0, 476.0, 2.0), BackgroundColor(white(0.3))));
            let enough = ctx.free || r.materials.iter().all(|(m, n)| ctx.inv.count(m) >= n * qty);
            button(w, abs(703.0, 790.0, 310.0, 44.0), if enough { "Start Production" } else { "Start Production (missing materials)" }, 13.0, Some(Action::Queue(e, r.id.clone(), qty)), Some(Tooltip { title: "Start Production".into(), body: "Takes the materials and queues the job. Hold F at the station to work it, or let camp creatures do it.".into() }));
            w.spawn((t("Tap F: this menu.  Hold F at the station: work on the queue.", 10.0, white(0.55)), Node { position_type: PositionType::Absolute, left: px(624.0), top: px(850.0), ..default() }));
        });
        // Production queue (beside the window).
        window(c, 1556.0, 218.0, 340.0, 560.0, |w| {
            header(w, 0.0, 0.0, 340.0, "Production Queue");
            let mut y = 44.0;
            if st.queue.is_empty() {
                w.spawn((t("Nothing queued.", 11.0, white(0.5)), Node { position_type: PositionType::Absolute, left: px(14.0), top: px(y), ..default() }));
                y += 28.0;
            }
            for (i, j) in st.queue.iter().enumerate() {
                let product = db.recipe(&j.recipe).map_or(j.recipe.clone(), |r| r.product.clone());
                w.spawn((Node { border: UiRect::all(px(1.0)), ..abs(10.0, y, 320.0, 58.0) }, BackgroundColor(hex("11190ECC")), BorderColor::all(white(0.2)))).with_children(|row| {
                    if let Some(h) = ctx.icons.item(db, &product) {
                        row.spawn((abs(4.0, 4.0, 48.0, 48.0), ImageNode::new(h)));
                    }
                    row.spawn((t(format!("{}  x{}", ctx.item_name(&product), j.remaining), 11.0, Color::WHITE), Node { position_type: PositionType::Absolute, left: px(60.0), top: px(6.0), ..default() }));
                    bar(row, abs(60.0, 32.0, 200.0, 12.0), j.work / j.need.max(1.0), hex("FF9100"), None);
                    button(row, abs(276.0, 12.0, 34.0, 34.0), "X", 11.0, Some(Action::Cancel(e, i)), Some(Tooltip { title: "Cancel".into(), body: "Cancel this job; materials come back.".into() }));
                });
                y += 64.0;
            }
            if !st.output.is_empty() {
                let got = st.output.iter().map(|(i, n)| format!("{} x{n}", ctx.item_name(i))).collect::<Vec<_>>().join(", ");
                button(w, abs(10.0, y + 6.0, 320.0, 44.0), format!("Collect: {got}"), 11.0, Some(Action::Collect(e)), None);
                y += 60.0;
            }
            w.spawn((t(format!("Work speed\n  you {:.0}/s while holding F\n  camp creatures {workers:.0}/s", crate::crafting::player_work_speed(db, p)), 11.0, white(0.7)), Node { position_type: PositionType::Absolute, left: px(14.0), top: px(y + 10.0), ..default() }));
        });
    }))
}

fn storage(world: &mut World, e: Entity, ctx: Ctx) -> Option<Box<dyn FnOnce(&mut C)>> {
    let s = world.get::<Storage>(e)?.clone();
    let name = world.get::<Building>(e).map_or("Storage".into(), |b| b.name.clone());
    Some(Box::new(move |c: &mut C| {
        window(c, 262.0, 180.0, 612.0, 752.0, |w| {
            header(w, 12.0, 20.0, 588.0, "Inventory");
            button(w, abs(476.0, 22.0, 120.0, 28.0), "Sort", 10.0, Some(Action::Sort), None);
            w.spawn((Node { display: Display::Grid, grid_template_columns: RepeatedGridTrack::px(6, 76.0), column_gap: px(10.0), row_gap: px(10.0), ..abs(53.0, 68.0, 506.0, 610.0) },)).with_children(|g| {
                for (i, st) in ctx.inv.bag.slots.iter().enumerate() {
                    slot(g, &ctx, Node::default(), 76.0, st.as_ref(), st.as_ref().map(|_| Action::Store(e, i)), "Click: put in the chest", false);
                }
            });
            w.spawn((t(format!("Weight {:.1} / {:.0}", ctx.inv.weight(&ctx.db).max(0.0), ctx.inv.max_weight(&ctx.db)), 11.0, hex(TEXT)), Node { position_type: PositionType::Absolute, left: px(53.0), top: px(700.0), ..default() }));
        });
        c.spawn((t("click\n<-  ->\nto move", 12.0, white(0.5)), TextLayout::justify(Justify::Center), Node { position_type: PositionType::Absolute, left: px(920.0), top: px(520.0), ..default() }));
        window(c, 1065.0, 180.0, 612.0, 752.0, |w| {
            header(w, 12.0, 20.0, 588.0, format!("{name}{}", if s.food_only { "   (food only)" } else { "" }));
            button(w, abs(476.0, 21.0, 120.0, 30.0), "Take All", 10.0, Some(Action::TakeAll(e)), None);
            w.spawn((Scroll(4), ctx.scroll(4), Node { overflow: Overflow::scroll_y(), display: Display::Grid, grid_template_columns: RepeatedGridTrack::px(6, 76.0), grid_auto_rows: vec![GridTrack::px(76.0)], column_gap: px(10.0), row_gap: px(10.0), ..abs(53.0, 68.0, 506.0, 620.0) })).with_children(|g| {
                for (i, st) in s.items.slots.iter().enumerate() {
                    slot(g, &ctx, Node::default(), 76.0, st.as_ref(), st.as_ref().map(|_| Action::Take(e, i)), "Click: take", false);
                }
            });
            let used = s.items.slots.iter().filter(|x| x.is_some()).count();
            w.spawn((t(format!("{used} / {} slots", s.items.slots.len()), 11.0, hex(TEXT)), Node { position_type: PositionType::Absolute, left: px(53.0), top: px(700.0), ..default() }));
        });
    }))
}

/// A creature as the screens show it.
#[derive(Clone)]
struct CreatureView {
    id: u64,
    species: String,
    name: String,
    level: u32,
    exp: u32,
    next: u32,
    hp: f32,
    max_hp: f32,
    food: f32,
    max_food: f32,
    mood: f32,
    attack: f32,
    defense: f32,
    craft: f32,
    elements: String,
    gender: String,
    partner: (String, String),
    works: Vec<(String, u32)>,
    skills: Vec<(u32, String, String, u8, String)>,
    passives: Vec<String>,
    place: Place,
    down: bool,
}

fn creature_view(db: &Db, p: &ck_core::creature::Individual) -> CreatureView {
    let sp = &db.creatures[&p.species];
    let st = p.stats(db.rules(), sp);
    let elements = sp.elements.iter().map(|e| e.name()).filter(|e| !e.is_empty()).collect::<Vec<_>>().join(" / ");
    let bond = sp.bond.as_ref().and_then(|b| db.bonds.get(b)).map_or((String::new(), String::new()), |b| (b.name.clone(), b.desc.clone()));
    CreatureView {
        id: p.id,
        species: p.species.clone(),
        name: p.name(sp).to_owned(),
        level: p.level,
        exp: p.exp,
        next: ck_core::creature::next_exp(db.rules(), p.level).unwrap_or(0),
        hp: p.hp,
        max_hp: st.max_hp,
        food: p.fullness,
        max_food: sp.stomach.max(1.0),
        mood: p.mood,
        attack: st.shot_attack,
        defense: st.defense,
        craft: sp.craft_speed,
        elements,
        gender: format!("{:?}", p.gender),
        partner: bond,
        works: crate::data::JOBS.iter().map(|s| (s.to_string(), sp.job_rank(s))).filter(|(_, r)| *r > 0).collect(),
        skills: sp
            .learnset
            .iter()
            .map(|(lv, w)| {
                let z = db.skills.get(w);
                let state = if p.equipped.contains(w) { 2 } else if p.learned.contains(w) { 1 } else { 0 };
                (*lv, z.map_or(w.clone(), |z| z.name.clone()), z.map_or(String::new(), |z| format!("{}  power {:.0}  cd {:.0}s", z.element.name(), z.power, z.cooldown)), state, w.clone())
            })
            .collect(),
        passives: vec![format!("HP talent {}", p.talents.hp), format!("Attack talent {}", p.talents.shot), format!("Defense talent {}", p.talents.defense)],
        place: p.place,
        down: p.incapacitated,
    }
}

fn portrait(c: &mut C, ctx: &Ctx, node: Node, species: &str, name: &str) {
    match ctx.icons.get(&format!("creature_{species}")) {
        Some(h) => {
            c.spawn((node, ImageNode::new(h)));
        }
        None => {
            c.spawn((Node { justify_content: JustifyContent::Center, align_items: AlignItems::Center, border_radius: BorderRadius::MAX, ..node }, BackgroundColor(white(0.15)))).with_children(|n| {
                n.spawn(t(name.chars().next().unwrap_or('?').to_string(), 16.0, Color::WHITE));
            });
        }
    }
}

/// A party card (348x80): Lv, name, HP bar with numbers, hunger bar, portrait at the right edge.
fn party_card(c: &mut C, ctx: &Ctx, x: f32, y: f32, i: usize, creature: Option<&CreatureView>, selected: bool) {
    let base = if selected { hex("FFAC2633") } else { hex("11190E99") };
    let edge = if selected { hex("FFAC26") } else { white(0.3) };
    let mut e = c.spawn((Node { border: UiRect::all(px(if selected { 2.0 } else { 1.0 })), ..abs(x, y, 348.0, 80.0) }, BackgroundColor(base), BorderColor::all(edge)));
    let Some(p) = creature else {
        e.with_children(|r| {
            r.spawn((t(format!("{}   empty", i + 1), 12.0, white(0.25)), Node { position_type: PositionType::Absolute, left: px(20.0), top: px(28.0), ..default() }));
        });
        return;
    };
    e.insert((Button, MenuButton(Action::Details(p.id)), Hoverable { base, hover: hex("FFAC2633"), edge }, Interaction::default(), Tooltip { title: p.name.clone(), body: "Click: show details".into() }));
    e.with_children(|r| {
        if p.down {
            r.spawn((abs(0.0, 0.0, 5.0, 78.0), BackgroundColor(hex("DC001D"))));
        }
        r.spawn((t("Lv", 10.0, white(0.8)), Node { position_type: PositionType::Absolute, left: px(20.0), top: px(10.0), ..default() }));
        r.spawn((t(p.level.to_string(), 16.0, hex("F5F4F4")), Node { position_type: PositionType::Absolute, left: px(40.0), top: px(4.0), ..default() }));
        r.spawn((t(format!("{}{}", p.name, if p.down { "  (down)" } else { "" }), 13.0, Color::WHITE), Node { position_type: PositionType::Absolute, left: px(76.0), top: px(6.0), ..default() }));
        bar(r, abs(20.0, 36.0, 212.0, 16.0), p.hp / p.max_hp.max(1.0), hex("41BF59"), Some(format!("{:.0} / {:.0}", p.hp, p.max_hp)));
        bar(r, abs(20.0, 58.0, 212.0, 6.0), p.food / p.max_food, hex("D87735"), None);
        portrait(r, ctx, abs(250.0, -8.0, 92.0, 92.0), &p.species, &p.name);
    });
}

/// The creature detail panel (520x840), with move buttons when `moves`.
fn creature_detail(c: &mut C, ctx: &Ctx, x: f32, y: f32, creature: Option<&CreatureView>, moves: bool) {
    window(c, x, y, 520.0, 840.0, |w| {
        let Some(p) = creature else {
            w.spawn((t("Select a creature to see its details.", 14.0, white(0.5)), Node { position_type: PositionType::Absolute, left: px(30.0), top: px(40.0), ..default() }));
            return;
        };
        // Title row: LEVEL + number, name, EXP bar.
        w.spawn((abs(23.0, 32.0, 3.0, 82.0), BackgroundColor(hex(ACCENT))));
        w.spawn((abs(30.0, 32.0, 100.0, 82.0), BackgroundColor(hex("958B7326"))));
        w.spawn((t("LEVEL", 11.0, white(0.7)), Node { position_type: PositionType::Absolute, left: px(56.0), top: px(36.0), ..default() }));
        w.spawn((t(p.level.to_string(), 36.0, Color::WHITE), TextLayout::justify(Justify::Center), Node { position_type: PositionType::Absolute, left: px(30.0), top: px(52.0), width: px(100.0), justify_content: JustifyContent::Center, ..default() }));
        w.spawn((abs(134.0, 32.0, 362.0, 82.0), BackgroundColor(hex("958B7326"))));
        w.spawn((t(&p.name, 18.0, Color::WHITE), Node { position_type: PositionType::Absolute, left: px(146.0), top: px(38.0), ..default() }));
        w.spawn((t(format!("{}   {}", p.elements, p.gender), 10.0, white(0.7)), Node { position_type: PositionType::Absolute, left: px(146.0), top: px(68.0), ..default() }));
        bar(w, abs(146.0, 92.0, 260.0, 4.0), p.exp as f32 / p.next.max(1) as f32, hex("FDAE2F"), None);
        w.spawn((t(format!("EXP {} / {}", p.exp, p.next), 9.0, hex("CCCCCC")), Node { position_type: PositionType::Absolute, left: px(146.0), top: px(98.0), ..default() }));
        // Portrait disc + bars.
        w.spawn((Node { border_radius: BorderRadius::MAX, ..abs(36.0, 136.0, 150.0, 150.0) }, BackgroundColor(hex("27302280"))));
        portrait(w, ctx, abs(46.0, 146.0, 130.0, 130.0), &p.species, &p.name);
        bar(w, abs(205.0, 150.0, 290.0, 24.0), p.hp / p.max_hp.max(1.0), hex("26BF73"), Some(format!("HP  {:.0} / {:.0}", p.hp, p.max_hp)));
        bar(w, abs(205.0, 182.0, 290.0, 24.0), p.food / p.max_food, hex("D77635"), Some(format!("Food  {:.0} / {:.0}", p.food, p.max_food)));
        bar(w, abs(205.0, 214.0, 290.0, 16.0), p.mood / 100.0, hex("FFC260"), Some(format!("Mood  {:.0} / 100", p.mood)));
        for (k, (label, v)) in [("Attack", p.attack), ("Defense", p.defense), ("Work Speed", p.craft)].iter().enumerate() {
            w.spawn((Node { justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Center, padding: UiRect::axes(px(8.0), px(0.0)), ..abs(205.0, 238.0 + k as f32 * 30.0, 290.0, 28.0) }, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.3)))).with_children(|r| {
                r.spawn(t(*label, 10.0, hex("E8E8E8")));
                r.spawn(t(format!("{v:.0}"), 14.0, Color::WHITE));
            });
        }
        // Job icons with talent ranks.
        let mut yy = 340.0;
        w.spawn((t("Jobs", 9.0, hex("D0922F")), Node { position_type: PositionType::Absolute, left: px(38.0), top: px(yy), ..default() }));
        w.spawn((Node { flex_wrap: FlexWrap::Wrap, column_gap: px(4.0), row_gap: px(2.0), padding: UiRect::all(px(4.0)), ..abs(38.0, yy + 18.0, 442.0, 84.0) }, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.3)))).with_children(|r| {
            for (s, rank) in &p.works {
                r.spawn((Node { width: px(44.0), height: px(37.0), ..default() }, Interaction::default(), Tooltip { title: crate::base_tools::suit_name(&ctx.db, s).to_owned(), body: format!("Talent {rank}: does {} work at stations and around the camp.", crate::work::verb(s)) })).with_children(|i| {
                    if let Some(h) = ctx.icons.get(&format!("work_{s}")) {
                        i.spawn((abs(0.0, 0.0, 34.0, 34.0), ImageNode::new(h)));
                    }
                    i.spawn((t(rank.to_string(), 11.0, Color::WHITE), TextShadow::default(), Node { position_type: PositionType::Absolute, right: px(2.0), bottom: px(-2.0), ..default() }));
                });
            }
        });
        yy += 108.0;
        // Bond skill.
        w.spawn((t("Bond Skill", 9.0, hex("D0922F")), Node { position_type: PositionType::Absolute, left: px(38.0), top: px(yy), ..default() }));
        w.spawn((Node { align_items: AlignItems::Center, padding: UiRect::left(px(10.0)), border: UiRect::left(px(3.0)), ..abs(38.0, yy + 16.0, 442.0, 32.0) }, BackgroundColor(hex("FF8D0026")), BorderColor::all(hex("FF8D00")), Interaction::default(), Tooltip { title: p.partner.0.clone(), body: wrap(&crate::ui::rich(&ctx.db, &p.partner.1), 60) })).with_children(|r| {
            r.spawn(t(if p.partner.0.is_empty() { "-".to_owned() } else { format!("{}   (F while out)", p.partner.0) }, 11.0, Color::WHITE));
        });
        yy += 54.0;
        // Active skills: click learned ones to equip / unequip.
        w.spawn((t(format!("Active Skills   (click to equip, {} max)", ck_core::creature::EQUIP_SLOTS), 9.0, hex("D0922F")), Node { position_type: PositionType::Absolute, left: px(38.0), top: px(yy), ..default() }));
        yy += 16.0;
        for (lv, name, info, state, id) in p.skills.iter().take(7) {
            let base = match state {
                2 => hex("FFAC2640"),
                1 => hex("11190ECC"),
                _ => hex("11190E66"),
            };
            let mut e = w.spawn((Node { align_items: AlignItems::Center, padding: UiRect::axes(px(10.0), px(0.0)), column_gap: px(10.0), border: UiRect::left(px(3.0)), ..abs(38.0, yy, 442.0, 27.0) }, BackgroundColor(base), BorderColor::all(if *state == 2 { hex(ACCENT) } else { white(0.15) }), Interaction::default(), Tooltip { title: name.clone(), body: format!("{info}\n{}", match state { 2 => "Equipped. Click to unequip.", 1 => "Learned. Click to equip.", _ => "Learned at that level." }) }));
            if *state > 0 {
                e.insert((Button, MenuButton(Action::Equip(p.id, id.clone())), Hoverable { base, hover: hex("FFAC2659"), edge: if *state == 2 { hex(ACCENT) } else { white(0.15) } }));
            }
            e.with_children(|r| {
                r.spawn(t(format!("Lv{lv}"), 9.0, white(0.5)));
                r.spawn((t(name, 11.0, white(if *state > 0 { 1.0 } else { 0.4 })), Node { flex_grow: 1.0, ..default() }));
                r.spawn(t(if *state == 2 { "EQUIPPED".to_owned() } else { info.split("  ").nth(1).unwrap_or("").to_owned() }, 9.0, if *state == 2 { hex(ACCENT) } else { white(0.5) }));
            });
            yy += 30.0;
        }
        // Talents 2x2.
        yy += 4.0;
        w.spawn((t("Talents", 9.0, hex("D0922F")), Node { position_type: PositionType::Absolute, left: px(38.0), top: px(yy), ..default() }));
        for k in 0..4 {
            let name = p.passives.get(k).cloned().unwrap_or_default();
            w.spawn((Node { align_items: AlignItems::Center, padding: UiRect::left(px(8.0)), ..abs(38.0 + (k % 2) as f32 * 222.0, yy + 16.0 + (k / 2) as f32 * 28.0, 218.0, 25.0) }, BackgroundColor(hex("0F150B")))).with_children(|r| {
                r.spawn(t(if name.is_empty() { "-".to_owned() } else { name }, 10.0, white(0.8)));
            });
        }
        if moves {
            let tip = |s: &str| Some(Tooltip { title: s.to_owned(), body: match s { "To Party" => "Join the party (5 slots). Summon with E.", "To Camp" => "Live and work at the camp (limited by the camp level; see the Camp Core).", _ => "Rest in the Camp Core's storage (recovers HP)." }.into() });
            let mut bx = 38.0;
            for (label, act, here) in [("To Party", Action::ToParty(p.id), matches!(p.place, Place::Party(_))), ("To Camp", Action::ToCamp(p.id), p.place == Place::Camp), ("To Storage", Action::ToStorage(p.id), p.place == Place::Storage)] {
                if !here {
                    button(w, abs(bx, 852.0, 140.0, 40.0), label, 12.0, Some(act), tip(label));
                    bx += 150.0;
                }
            }
        }
    });
}

fn creature_views(world: &mut World) -> (Vec<Option<CreatureView>>, Vec<CreatureView>, Vec<CreatureView>) {
    let db = world.resource::<Db>().clone();
    let roster = world.resource::<Creatures>();
    let view = |id: u64| roster.0.get(id).map(|p| creature_view(&db, p));
    let party = roster.0.party().iter().take(PARTY_SIZE).map(|s| s.and_then(view)).collect();
    let boxed = roster.0.in_place(Place::Storage).into_iter().filter_map(view).collect();
    let base = roster.0.in_place(Place::Camp).into_iter().filter_map(view).collect();
    (party, boxed, base)
}

fn party(world: &mut World, ctx: Ctx, id: u64) -> Box<dyn FnOnce(&mut C)> {
    let (party, ..) = creature_views(world);
    Box::new(move |c: &mut C| {
        window(c, 380.0, 120.0, 420.0, 600.0, |w| {
            header(w, 20.0, 20.0, 380.0, "Party");
            for (i, p) in party.iter().enumerate() {
                party_card(w, &ctx, 36.0, 66.0 + i as f32 * 94.0, i, p.as_ref(), p.as_ref().is_some_and(|p| p.id == id));
            }
        });
        if party.iter().all(Option::is_none) {
            c.spawn((t("No creatures in party!\nCatch one: hold Q to aim a Capture Orb, release to throw.", 18.0, white(0.6)), Node { position_type: PositionType::Absolute, left: px(900.0), top: px(400.0), ..default() }));
            return;
        }
        let sel = party.iter().flatten().find(|p| p.id == id).or(party.iter().flatten().next());
        creature_detail(c, &ctx, 860.0, 110.0, sel, false);
        c.spawn((t("Move creatures between the party, the box and the camp at the Camp Core.", 10.0, white(0.5)), Node { position_type: PositionType::Absolute, left: px(380.0), top: px(740.0), ..default() }));
    })
}

fn camp_core(world: &mut World, ctx: Ctx) -> Box<dyn FnOnce(&mut C)> {
    let (party, boxed, base) = creature_views(world);
    let sel_id = world.resource::<MenuState>().sel_creature;
    let base_info = {
        let db = world.resource::<Db>().clone();
        let level = world.query::<&crate::building::BaseCamp>().iter(world).map(|b| b.level).max().unwrap_or(1);
        let ids: Vec<String> = world.query_filtered::<&Building, Without<crate::building::Construction>>().iter(world).map(|b| b.id.clone()).collect();
        let missions = crate::base::base_missions(&db, level, base.len() as u32, &ids);
        let next = crate::base::has_level(&db, level + 1).then(|| crate::base::max_workers(&db, level + 1));
        (level, crate::base::max_workers(&db, level), missions, next)
    };
    Box::new(move |c: &mut C| {
        let all: Vec<&CreatureView> = party.iter().flatten().chain(boxed.iter()).chain(base.iter()).collect();
        let sel = sel_id.and_then(|id| all.iter().find(|p| p.id == id).copied());
        // Party column.
        window(c, 170.0, 100.0, 430.0, 646.0, |w| {
            header(w, 30.0, 30.0, 370.0, "Party");
            for (i, p) in party.iter().enumerate() {
                party_card(w, &ctx, 44.0, 76.0 + i as f32 * 94.0, i, p.as_ref(), p.as_ref().is_some_and(|p| Some(p.id) == sel_id));
            }
        });
        // Box grid, 6x5 cells of 80 with cyan corner markers.
        let cell = |g: &mut C, p: &CreatureView, x: f32, y: f32| {
            let on = Some(p.id) == sel_id;
            let base = if on { hex("FFAC2633") } else { Color::NONE };
            let edge = if on { hex("FFAC26") } else { Color::NONE };
            g.spawn((Button, MenuButton(Action::Details(p.id)), Hoverable { base, hover: hex("FFAC2633"), edge }, Node { border: UiRect::all(px(2.0)), ..abs(x, y, 80.0, 80.0) }, BackgroundColor(base), BorderColor::all(edge), Tooltip { title: format!("{} Lv{}", p.name, p.level), body: "Click: details and move buttons".into() })).with_children(|b| {
                portrait(b, &ctx, abs(6.0, 4.0, 64.0, 64.0), &p.species, &p.name);
                b.spawn((t(format!("Lv{}", p.level), 9.0, Color::WHITE), TextShadow::default(), Node { position_type: PositionType::Absolute, left: px(4.0), bottom: px(2.0), ..default() }));
                if p.down {
                    b.spawn((abs(0.0, 0.0, 4.0, 76.0), BackgroundColor(hex("DC001D"))));
                }
            });
        };
        window(c, 600.0, 100.0, 600.0, 510.0, |w| {
            header(w, 40.0, 30.0, 520.0, format!("Box 1    ({} creatures)", boxed.len()));
            for i in 1..6 {
                for j in 1..5 {
                    w.spawn((abs(60.0 + i as f32 * 80.0 - 5.0, 93.0 + j as f32 * 80.0 - 1.0, 10.0, 2.0), BackgroundColor(hex(ACCENT))));
                    w.spawn((abs(60.0 + i as f32 * 80.0 - 1.0, 93.0 + j as f32 * 80.0 - 5.0, 2.0, 10.0), BackgroundColor(hex(ACCENT))));
                }
            }
            for (k, p) in boxed.iter().take(30).enumerate() {
                cell(w, p, 60.0 + (k % 6) as f32 * 80.0, 93.0 + (k / 6) as f32 * 80.0);
            }
        });
        // Camp workers, camp level and its goals.
        window(c, 600.0, 620.0, 600.0, 345.0, |w| {
            let (level, limit, missions, next) = &base_info;
            header(w, 30.0, 20.0, 540.0, format!("Camp creatures   {} / {limit}        Camp Lv {level}", base.len()));
            for (k, p) in base.iter().take(6).enumerate() {
                cell(w, p, 46.0 + k as f32 * 84.0, 62.0);
            }
            if base.is_empty() {
                w.spawn((t("No workers yet: select a creature and press To Camp.", 11.0, white(0.5)), Node { position_type: PositionType::Absolute, left: px(46.0), top: px(90.0), ..default() }));
            }
            let mut lines = Vec::new();
            match next {
                Some(n) if !missions.is_empty() => {
                    lines.push(format!("Camp Lv {} (allows {n} workers) needs:", level + 1));
                    for m in missions {
                        lines.push(format!("{}  {}   {}/{}", if m.have >= m.need { "[x]" } else { "[  ]" }, m.what, m.have.min(m.need), m.need));
                    }
                }
                _ => lines.push("Max camp level.".into()),
            }
            w.spawn((t(lines.join("\n"), 11.0, hex(TEXT)), Node { position_type: PositionType::Absolute, left: px(46.0), top: px(156.0), width: px(520.0), ..default() }));
        });
        creature_detail(c, &ctx, 1210.0, 110.0, sel, true);
    })
}

/// Esc menu: display mode, sound, resume, quit.
fn settings_screen(world: &World) -> Box<dyn FnOnce(&mut C)> {
    let full = world.resource::<crate::settings::Settings>().fullscreen;
    let muted = crate::sfx::is_muted(world);
    let free = world.resource::<crate::crafting::FreeCraft>().0;
    Box::new(move |c: &mut C| {
        window(c, 660.0, 260.0, 600.0, 560.0, |w| {
            header(w, 12.0, 20.0, 576.0, "Settings");
            let label = |w: &mut C, y: f32, s: &str| {
                w.spawn((t(s, 13.0, hex(TEXT)), Node { position_type: PositionType::Absolute, left: px(36.0), top: px(y), ..default() }));
            };
            // The current choice is lit (blue, no click); the other one is a normal button.
            let choice = |w: &mut C, x: f32, y: f32, text: &str, on: bool, act: Action| {
                if on {
                    w.spawn((Node { border: UiRect::all(px(1.0)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..abs(x, y, 170.0, 40.0) }, BackgroundColor(hex("FFAC268C")), BorderColor::all(white(0.9)))).with_children(|b| {
                        b.spawn(t(text, 13.0, hex(TEXT)));
                    });
                } else {
                    button(w, abs(x, y, 170.0, 40.0), text, 13.0, Some(act), None);
                }
            };
            label(w, 92.0, "Display");
            choice(w, 200.0, 82.0, "Fullscreen", full, Action::Fullscreen(true));
            choice(w, 390.0, 82.0, "Windowed", !full, Action::Fullscreen(false));
            label(w, 162.0, "Sound");
            choice(w, 200.0, 152.0, "On", !muted, Action::Sound(true));
            choice(w, 390.0, 152.0, "Off", muted, Action::Sound(false));
            label(w, 232.0, "Free mode");
            choice(w, 200.0, 222.0, "On", free, Action::Free(true));
            choice(w, 390.0, 222.0, "Off", !free, Action::Free(false));
            w.spawn((t("Free mode: crafting, building and technology cost nothing (no points or level needed).\nSaved for the next start. F2 toggles free mode, F3 sound.", 11.0, white(0.55)), Node { position_type: PositionType::Absolute, left: px(36.0), top: px(280.0), ..default() }));
            button(w, abs(150.0, 370.0, 300.0, 46.0), "Resume (Esc)", 14.0, Some(Action::Close), None);
            button(w, abs(150.0, 436.0, 300.0, 46.0), "Quit Game", 14.0, Some(Action::Quit), None);
            w.spawn((t("Quitting doesn't save: F5 quick-saves first.", 11.0, white(0.55)), Node { position_type: PositionType::Absolute, left: px(150.0), top: px(494.0), ..default() }));
        });
    })
}

/// Repair Bench: every damaged item with its cost, one button each and Repair All.
fn repair_screen(ctx: Ctx) -> Box<dyn FnOnce(&mut C)> {
    Box::new(move |c: &mut C| {
        let list = crate::base_tools::damaged(&ctx.db, &ctx.inv);
        window(c, 560.0, 160.0, 800.0, 760.0, |w| {
            header(w, 12.0, 20.0, 776.0, "Repair Bench");
            if list.is_empty() {
                w.spawn((t("Nothing needs repairing. Tools, weapons and armour wear down with use.", 12.0, white(0.7)), Node { position_type: PositionType::Absolute, left: px(30.0), top: px(80.0), ..default() }));
            }
            for (k, (label, item, dur, cost)) in list.iter().take(12).enumerate() {
                let y = 70.0 + k as f32 * 52.0;
                let max = ctx.db.item(item).and_then(|i| i.durability).unwrap_or(100.0);
                if let Some(h) = ctx.icons.item(&ctx.db, item) {
                    w.spawn((abs(24.0, y, 44.0, 44.0), ImageNode::new(h)));
                }
                w.spawn((t(format!("{}  ({label})", ctx.item_name(item)), 12.0, hex(TEXT)), Node { position_type: PositionType::Absolute, left: px(80.0), top: px(y + 2.0), ..default() }));
                bar(w, abs(80.0, y + 24.0, 220.0, 12.0), dur / max.max(1.0), hex("FF9100"), Some(format!("{dur:.0} / {max:.0}")));
                let costs = if cost.is_empty() { "free".to_owned() } else { cost.iter().map(|(m, n)| format!("{} {}/{n}", ctx.item_name(m), ctx.inv.count(m))).collect::<Vec<_>>().join("   ") };
                let enough = ctx.free || cost.iter().all(|(m, n)| ctx.inv.count(m) >= *n);
                w.spawn((t(costs, 11.0, if enough { white(0.8) } else { hex("FF6B5B") }), Node { position_type: PositionType::Absolute, left: px(320.0), top: px(y + 14.0), ..default() }));
                button(w, abs(650.0, y + 6.0, 120.0, 32.0), "Repair", 11.0, enough.then(|| Action::Repair(Some(label.clone()))), None);
            }
            if !list.is_empty() {
                button(w, abs(570.0, 700.0, 200.0, 40.0), "Repair All", 13.0, Some(Action::Repair(None)), None);
            }
            w.spawn((t("Cost: half the item's recipe, scaled by the missing durability. Free mode (F2) applies.", 10.0, white(0.5)), Node { position_type: PositionType::Absolute, left: px(24.0), top: px(712.0), ..default() }));
        });
    })
}

/// A camp creature as the Job Board shows it.
struct BoardCreature {
    id: u64,
    name: String,
    level: u32,
    species: String,
    /// (job, rank, priority number) for the jobs it has.
    suits: Vec<(&'static str, u32, u8)>,
    fixed: Option<Entity>,
    note: String,
}

/// A camp building with work for creatures.
struct BoardFacility {
    e: Entity,
    b: Building,
    blueprint: bool,
    ranch: bool,
    needs: String,
}

/// Job Board: "Job Priorities" (tab 0: a priority number per creature and job, 1 first, 0 never) and
/// "Assignments" (tab 1: assign camp creatures to buildings).
fn work_board(world: &mut World, ctx: Ctx) -> Box<dyn FnOnce(&mut C)> {
    let db = ctx.db.clone();
    let jobs: Vec<(&'static str, String)> = crate::data::JOBS.iter().map(|j| (*j, crate::base_tools::suit_name(&db, j).to_owned())).collect();
    let sel = world.resource::<MenuState>().sel_ent;
    let mut tasks: std::collections::HashMap<u64, (Option<Entity>, String)> = std::collections::HashMap::new();
    let mut q = world.query::<(&crate::creatures::CreatureActor, &crate::base::WorkerTask, &crate::creatures::Brain)>();
    for (a, t, b) in q.iter(world) {
        tasks.insert(a.id, (t.forced, b.note.clone()));
    }
    let roster_w = &world.resource::<Creatures>().0;
    let roster: Vec<BoardCreature> = roster_w
        .in_place(ck_core::creature::Place::Camp)
        .iter()
        .filter_map(|id| roster_w.get(*id))
        .map(|p| {
            let sp = db.creatures.get(&p.species);
            let suits = crate::data::JOBS.iter().filter_map(|s| sp.map(|sp| sp.job_rank(s)).filter(|r| *r > 0).map(|r| (*s, r, p.work_priority(s)))).collect();
            let (fixed, note) = tasks.get(&p.id).cloned().unwrap_or((None, "in storage".into()));
            BoardCreature { id: p.id, name: sp.map_or(p.species.clone(), |s| s.name.clone()), level: p.level, species: p.species.clone(), suits, fixed, note }
        })
        .collect();
    let base = world.query::<(&crate::building::BaseCamp, &GlobalTransform)>().iter(world).next().map(|(bc, t)| (t.translation(), bc.radius));
    let mut fq = world.query_filtered::<(Entity, &Building, &GlobalTransform, Has<crate::building::Construction>, Has<crate::base::Ranch>), Or<(With<Station>, With<crate::work::Producer>, With<crate::work::Farm>, With<crate::base::Ranch>, With<crate::building::Construction>)>>();
    let facilities: Vec<BoardFacility> = fq
        .iter(world)
        .filter(|(_, _, t, ..)| base.is_some_and(|(c, r)| t.translation().with_y(0.0).distance(c.with_y(0.0)) < r))
        .map(|(e, b, _, blueprint, ranch)| {
            let needs = if blueprint {
                format!("{} (building)", crate::base_tools::suit_name(&db, "craft"))
            } else if ranch {
                crate::base_tools::suit_name(&db, "ranch").to_owned()
            } else {
                crate::work::assign_rows(&db, &b.id).iter().map(|r| format!("{} Lv{}", crate::base_tools::suit_name(&db, &r.job), r.rank)).collect::<Vec<_>>().join(", ")
            };
            BoardFacility { e, b: b.clone(), blueprint, ranch, needs }
        })
        .collect();
    let name_of = |e: Entity| facilities.iter().find(|f| f.e == e).map_or_else(|| "?".to_owned(), |f| f.b.name.clone());
    let fixed_names: Vec<(u64, Option<String>)> = roster.iter().map(|p| (p.id, p.fixed.map(name_of))).collect();
    Box::new(move |c: &mut C| {
        let tab = ctx.tab.min(1);
        for (i, label) in ["Job Priorities", "Assignments"].into_iter().enumerate() {
            let on = i == tab;
            let base = if on { hex("C6BEAC") } else { hex("11190E99") };
            c.spawn((Button, MenuButton(Action::Tab(i)), Hoverable { base, hover: hex("FFAC2659"), edge: Color::NONE }, Node { justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..abs(160.0 + i as f32 * 248.0, 40.0, 240.0, 40.0) }, BackgroundColor(base))).with_children(|b| {
                b.spawn(t(label, 13.0, if on { hex("11190E") } else { hex(TEXT) }));
            });
        }
        let fixed_of = |id: u64| fixed_names.iter().find(|(p, _)| *p == id).and_then(|(_, n)| n.clone());
        if tab == 0 {
            window(c, 160.0, 90.0, 1600.0, 920.0, |w| {
                header(w, 12.0, 20.0, 1576.0, "Job Priorities");
                w.spawn((t("1 = most important, done first; 5 = least (everything starts at 5). Click: 5 -> 1 -> 2 -> 3 -> 4 -> Off -> 5. Off: never.", 11.0, white(0.65)), Node { position_type: PositionType::Absolute, left: px(24.0), top: px(60.0), ..default() }));
                for (k, (id, name)) in jobs.iter().enumerate() {
                    let x = 330.0 + k as f32 * 88.0;
                    if let Some(h) = ctx.icons.get(&format!("work_{id}")) {
                        w.spawn((abs(x + 22.0, 90.0, 40.0, 34.0), ImageNode::new(h), Interaction::default(), Tooltip { title: name.clone(), body: String::new() }));
                    }
                }
                if roster.is_empty() {
                    w.spawn((t("No creatures at the camp. Send some there from the Camp Core.", 12.0, white(0.7)), Node { position_type: PositionType::Absolute, left: px(24.0), top: px(150.0), ..default() }));
                }
                for (r, p) in roster.iter().take(13).enumerate() {
                    let y = 136.0 + r as f32 * 58.0;
                    w.spawn((abs(12.0, y - 2.0, 1576.0, 54.0), BackgroundColor(white(if r % 2 == 0 { 0.04 } else { 0.0 }))));
                    w.spawn((t(format!("{}  Lv{}", p.name, p.level), 13.0, hex(TEXT)), Node { position_type: PositionType::Absolute, left: px(24.0), top: px(y + 2.0), ..default() }));
                    let status = fixed_of(p.id).map_or_else(|| "Automatic".to_owned(), |f| format!("Assigned: {f}"));
                    w.spawn((t(status, 10.0, if p.fixed.is_some() { hex("FF9100") } else { white(0.5) }), Node { position_type: PositionType::Absolute, left: px(24.0), top: px(y + 24.0), ..default() }));
                    for (k, (id, _)) in jobs.iter().enumerate() {
                        let x = 330.0 + k as f32 * 88.0;
                        match p.suits.iter().find(|(s, ..)| s == id) {
                            Some((_, rank, prio)) => {
                                let (label, col) = match *prio {
                                    0 => ("Off".to_owned(), hex("FA3F3F40")),
                                    1 => ("1".to_owned(), hex("FF9100B0")),
                                    p if p >= ck_core::creature::DEFAULT_WORK_PRIO => (p.to_string(), hex("FFF6E214")),
                                    p => (p.to_string(), hex("FFC26040")),
                                };
                                let tip = Tooltip { title: format!("{} Lv{rank}", crate::base_tools::suit_name(&db, id)), body: "1 = most important. Click: 5 -> 1 -> 2 -> 3 -> 4 -> Off -> 5".into() };
                                c_cell(w, abs(x + 6.0, y + 4.0, 76.0, 40.0), label, col, Action::CreaturePrio(p.id, (*id).to_owned()), tip, *rank);
                            }
                            None => {
                                w.spawn((t("-", 12.0, white(0.2)), Node { position_type: PositionType::Absolute, left: px(x + 40.0), top: px(y + 14.0), ..default() }));
                            }
                        }
                    }
                    if p.fixed.is_some() {
                        button(w, abs(1480.0, y + 6.0, 100.0, 36.0), "Cancel\nAssignment", 9.0, Some(Action::Unfix(p.id)), None);
                    }
                }
            });
            return;
        }
        // Assignment Board.
        window(c, 160.0, 90.0, 760.0, 920.0, |w| {
            header(w, 12.0, 20.0, 736.0, "Camp Facilities");
            w.spawn((t("Pick a facility, then Assign a creature to it.", 11.0, white(0.65)), Node { position_type: PositionType::Absolute, left: px(24.0), top: px(60.0), ..default() }));
            for (r, f) in facilities.iter().take(14).enumerate() {
                let y = 90.0 + r as f32 * 58.0;
                let on = sel == Some(f.e);
                let base = if on { hex("FFC26040") } else { white(if r % 2 == 0 { 0.05 } else { 0.02 }) };
                let fixed: Vec<&str> = roster.iter().filter(|p| p.fixed == Some(f.e)).map(|p| p.name.as_str()).collect();
                w.spawn((Button, MenuButton(Action::PickFacility(f.e)), Hoverable { base, hover: hex("FFAC2640"), edge: white(if on { 0.9 } else { 0.1 }) }, Node { border: UiRect::all(px(1.0)), ..abs(12.0, y, 736.0, 54.0) }, BackgroundColor(base), BorderColor::all(white(if on { 0.9 } else { 0.1 })))).with_children(|row| {
                    if let Some(h) = ctx.icons.build(&f.b.id) {
                        row.spawn((abs(6.0, 5.0, 44.0, 44.0), ImageNode::new(h)));
                    }
                    row.spawn((t(f.b.name.clone(), 13.0, hex(TEXT)), Node { position_type: PositionType::Absolute, left: px(60.0), top: px(4.0), ..default() }));
                    row.spawn((t(format!("Required job: {}", f.needs), 10.0, white(0.55)), Node { position_type: PositionType::Absolute, left: px(60.0), top: px(28.0), ..default() }));
                    let who = if fixed.is_empty() { String::new() } else { format!("Fixed: {}", fixed.join(", ")) };
                    row.spawn((t(who, 10.0, hex("FF9100")), Node { position_type: PositionType::Absolute, left: px(470.0), top: px(18.0), ..default() }));
                });
            }
            if facilities.is_empty() {
                w.spawn((t("No facilities with work at the camp.", 12.0, white(0.7)), Node { position_type: PositionType::Absolute, left: px(24.0), top: px(100.0), ..default() }));
            }
        });
        let picked = sel.and_then(|e| facilities.iter().find(|f| f.e == e));
        window(c, 960.0, 90.0, 800.0, 920.0, |w| {
            header(w, 12.0, 20.0, 776.0, picked.map_or_else(|| "Camp creatures".to_owned(), |f| format!("Camp creatures  ->  {}", f.b.name)));
            for (r, p) in roster.iter().take(14).enumerate() {
                let y = 70.0 + r as f32 * 58.0;
                w.spawn((abs(12.0, y, 776.0, 54.0), BackgroundColor(white(if r % 2 == 0 { 0.05 } else { 0.02 }))));
                w.spawn((t(format!("{}  Lv{}", p.name, p.level), 13.0, hex(TEXT)), Node { position_type: PositionType::Absolute, left: px(24.0), top: px(y + 4.0), ..default() }));
                for (k, (s, rank, _)) in p.suits.iter().enumerate() {
                    if let Some(h) = ctx.icons.get(&format!("work_{s}")) {
                        w.spawn((abs(24.0 + k as f32 * 46.0, y + 28.0, 24.0, 20.0), ImageNode::new(h)));
                        w.spawn((t(rank.to_string(), 9.0, white(0.8)), Node { position_type: PositionType::Absolute, left: px(50.0 + k as f32 * 46.0), top: px(y + 30.0), ..default() }));
                    }
                }
                let status = match fixed_of(p.id) {
                    Some(f) => format!("Fixed: {f}"),
                    None => "Automatic".to_owned(),
                };
                w.spawn((t(status, 10.0, if p.fixed.is_some() { hex("FF9100") } else { white(0.5) }), Node { position_type: PositionType::Absolute, left: px(330.0), top: px(y + 6.0), ..default() }));
                w.spawn((t(p.note.chars().take(38).collect::<String>(), 9.0, white(0.45)), Node { position_type: PositionType::Absolute, left: px(330.0), top: px(y + 28.0), ..default() }));
                if let Some(f) = picked {
                    let there = roster.iter().filter(|x| x.fixed == Some(f.e)).count();
                    let ok = p.fixed != Some(f.e) && crate::base_tools::check_fixed(&ctx.db, &p.species, &f.b, f.blueprint, f.ranch, there).is_ok();
                    button(w, abs(640.0, y + 8.0, 70.0, 36.0), "Assign", 10.0, ok.then_some(Action::FixCreature(p.id, f.e)), None);
                }
                if p.fixed.is_some() {
                    button(w, abs(716.0, y + 8.0, 66.0, 36.0), "Cancel", 10.0, Some(Action::Unfix(p.id)), None);
                }
            }
        });
    })
}

/// A clickable priority cell with the talent rank in the corner.
fn c_cell(c: &mut C, node: Node, label: String, base: Color, act: Action, tip: Tooltip, rank: u32) {
    c.spawn((Button, MenuButton(act), Hoverable { base, hover: hex("FFAC2659"), edge: white(0.3) }, Interaction::default(), tip, Node { border: UiRect::all(px(1.0)), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..node }, BackgroundColor(base), BorderColor::all(white(0.3)))).with_children(|b| {
        b.spawn(t(label, 15.0, Color::WHITE));
        b.spawn((t(format!("Lv{rank}"), 8.0, white(0.55)), Node { position_type: PositionType::Absolute, right: px(3.0), bottom: px(1.0), ..default() }));
    });
}
