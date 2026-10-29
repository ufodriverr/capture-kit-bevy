//! Shared UI look: the palette, panels, item slots with icons, tabs and buttons used by the HUD and the
//! menus. Icons are original images generated for this project (`assets/ui/icons`, tools/icons).

use std::collections::HashMap;

use bevy::prelude::*;
use bevy::text::FontSize;
use bevy::ui::RelativeCursorPosition;

use crate::data::Db;

/// The Capture Kit palette: warm parchment text on dark moss panels, amber accents.
pub mod col {
    use bevy::prelude::Color;
    /// Body text.
    pub const TEXT: Color = Color::srgb(0.96, 0.93, 0.84);
    /// Accent: borders, selection, headings.
    pub const ACCENT: Color = Color::srgb(0.96, 0.69, 0.25);
    /// Panel fill.
    pub const PANEL: Color = Color::srgba(0.07, 0.10, 0.07, 0.78);
    /// Success (mission done).
    pub const GOOD: Color = Color::srgb(0.55, 0.86, 0.42);
    pub const GOOD_BG: Color = Color::srgba(0.12, 0.25, 0.10, 0.82);
}

/// Icon handles by id (item id, `build_<id>`, `work_<job>`, `stat_<x>`).
#[derive(Resource, Debug, Default, Clone)]
pub struct Icons(pub HashMap<String, Handle<Image>>);

impl Icons {
    /// The icon for an item: its `icon` (items.ron, default the id), else one for its kind.
    pub fn item(&self, db: &Db, id: &str) -> Option<Handle<Image>> {
        let it = db.item(id);
        if let Some(h) = self.0.get(it.map_or(id, |i| i.icon())) {
            return Some(h.clone());
        }
        let fallback = match it.map(|i| (i.kind.as_str(), i.sub.as_str())) {
            Some(("Orb", _)) => "capture_orb",
            Some(("Ammo", _)) => "arrow",
            Some(("Armor", _)) => "cloth_tunic",
            Some(("Food", _)) => "grilled_meat",
            _ => return None,
        };
        self.0.get(fallback).cloned()
    }

    pub fn build(&self, id: &str) -> Option<Handle<Image>> {
        self.0.get(&format!("build_{id}")).cloned()
    }

    pub fn get(&self, id: &str) -> Option<Handle<Image>> {
        self.0.get(id).cloned()
    }
}

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Icons>().init_resource::<ScrollMemory>().add_systems(PreStartup, load_icons).add_systems(Startup, spawn_tip).add_systems(Update, (hover, tooltip, scroll));
    }
}

fn load_icons(assets: Res<AssetServer>, mut icons: ResMut<Icons>) {
    let dir = crate::data::assets_dir().join("ui/icons");
    let Ok(rd) = std::fs::read_dir(&dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().is_some_and(|x| x == "png")
            && let Some(stem) = p.file_stem().and_then(|s| s.to_str())
        {
            icons.0.insert(stem.to_owned(), assets.load(format!("ui/icons/{stem}.png")));
        }
    }
}

/// Buttons light up under the cursor (fill and frame), as the item slot's focus state.
#[derive(Component, Debug, Clone, Copy)]
pub struct Hoverable {
    pub base: Color,
    pub hover: Color,
    pub edge: Color,
}

fn hover(mut q: Query<(&Interaction, &Hoverable, &mut BackgroundColor, Option<&mut BorderColor>), Changed<Interaction>>) {
    for (i, h, mut bg, border) in &mut q {
        let on = matches!(i, Interaction::Hovered | Interaction::Pressed);
        bg.0 = if on { h.hover } else { h.base };
        if let Some(mut b) = border {
            *b = BorderColor::all(if on { col::ACCENT } else { h.edge });
        }
    }
}

/// Hover text for a button: a title and body lines, shown in a panel beside the cursor.
#[derive(Component, Debug, Clone, Default)]
pub struct Tooltip {
    pub title: String,
    pub body: String,
}

#[derive(Component, Debug)]
struct TipPanel;

#[derive(Component, Debug)]
struct TipTitle;

#[derive(Component, Debug)]
struct TipBody;

fn spawn_tip(mut commands: Commands) {
    commands
        .spawn((TipPanel, Node { position_type: PositionType::Absolute, max_width: px(420.0), flex_direction: FlexDirection::Column, padding: UiRect::all(px(10.0)), row_gap: px(6.0), border: UiRect::all(px(1.0)), ..default() }, BackgroundColor(Color::srgba(0.06, 0.08, 0.05, 0.95)), BorderColor::all(col::ACCENT.with_alpha(0.45)), Visibility::Hidden, GlobalZIndex(60), Pickable::IGNORE))
        .with_children(|t| {
            t.spawn((text("", 18.0, col::TEXT), TipTitle, Node { padding: UiRect::bottom(px(4.0)), border: UiRect::bottom(px(2.0)), ..default() }, BorderColor::all(col::ACCENT.with_alpha(0.6))));
            t.spawn((text("", 15.0, col::TEXT.with_alpha(0.85)), TipBody));
        });
}

#[allow(clippy::type_complexity)]
fn tooltip(windows: Query<&Window, With<bevy::window::PrimaryWindow>>, scale: Res<UiScale>, tips: Query<(&Interaction, &Tooltip)>, mut panel: Query<(&mut Node, &mut Visibility), With<TipPanel>>, mut title: Query<&mut Text, (With<TipTitle>, Without<TipBody>)>, mut body: Query<&mut Text, (With<TipBody>, Without<TipTitle>)>) {
    let Ok((mut node, mut vis)) = panel.single_mut() else { return };
    let hovered = tips.iter().find(|(i, t)| **i == Interaction::Hovered && !t.title.is_empty()).map(|(_, t)| t);
    let cursor = windows.single().ok().and_then(|w| w.cursor_position().map(|c| (c, w.width(), w.height())));
    let (Some(t), Some((c, w, h))) = (hovered, cursor) else {
        *vis = Visibility::Hidden;
        return;
    };
    *vis = Visibility::Inherited;
    let (x, y) = (c.x / scale.0, c.y / scale.0);
    let (w, h) = (w / scale.0, h / scale.0);
    // Right of the cursor, flipped left / up near the edges.
    node.left = px(if x + 440.0 > w { (x - 440.0).max(0.0) } else { x + 24.0 });
    node.top = px(if y + 260.0 > h { (y - 260.0).max(0.0) } else { y + 16.0 });
    if let Ok(mut s) = title.single_mut()
        && s.0 != t.title
    {
        s.0 = t.title.clone();
    }
    if let Ok(mut s) = body.single_mut()
        && s.0 != t.body
    {
        s.0 = t.body.clone();
    }
}

/// A scrollable list (mouse wheel over it). The offset is kept per key across menu rebuilds.
#[derive(Component, Debug, Clone, Copy)]
#[require(ScrollPosition, RelativeCursorPosition)]
pub struct Scroll(pub u8);

#[derive(Resource, Debug, Default)]
pub struct ScrollMemory(pub HashMap<u8, f32>);

fn scroll(mut wheel: MessageReader<bevy::input::mouse::MouseWheel>, mut mem: ResMut<ScrollMemory>, mut q: Query<(&Scroll, &RelativeCursorPosition, &mut ScrollPosition, &ComputedNode)>) {
    let dy: f32 = wheel.read().map(|w| if w.unit == bevy::input::mouse::MouseScrollUnit::Line { w.y * 60.0 } else { w.y }).sum();
    for (s, rel, mut pos, cn) in &mut q {
        if dy != 0.0 && rel.cursor_over() {
            let max = ((cn.content_size().y - cn.size().y) * cn.inverse_scale_factor()).max(0.0);
            pos.0.y = (pos.0.y - dy).clamp(0.0, max);
        }
        mem.0.insert(s.0, pos.0.y);
    }
}

/// Rich text in content descriptions to plain text: `<itemName id=|X|/>`, `<buildName id=|X|/>` and
/// `<creatureName id=|X|/>` become names, other tags are dropped.
pub fn rich(db: &Db, s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('<') {
        out.push_str(&rest[..i]);
        let Some(j) = rest[i..].find('>') else { break };
        let tag = &rest[i..i + j];
        let id = tag.split("id=|").nth(1).and_then(|t| t.split('|').next()).unwrap_or("");
        let l = tag.to_ascii_lowercase();
        if l.starts_with("<itemname") {
            out.push_str(db.item(id).map_or(id, |x| x.name.as_str()));
        } else if l.starts_with("<buildname") {
            out.push_str(db.build(id).map_or(id, |b| b.name.as_str()));
        } else if l.starts_with("<creaturename") {
            out.push_str(db.creatures.get(id).map_or(id, |x| x.name.as_str()));
        }
        rest = &rest[i + j + 1..];
    }
    out.push_str(rest);
    out.replace('\r', "")
}

pub fn text(s: impl Into<String>, size: f32, color: Color) -> (Text, TextFont, TextColor) {
    (Text::new(s), TextFont { font_size: FontSize::Px(size), ..default() }, TextColor(color))
}
