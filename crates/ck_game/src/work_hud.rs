//! Progress above working facilities, a box above the station: product icon, name x left, progress bar,
//! time left at the current work rate, and the number of workers. Stations show "Ready" when output waits,
//! fields show their stage. Hidden while a menu is open and beyond 15 m.

use std::collections::HashMap;

use bevy::prelude::*;

use crate::building::{Building, Construction};
use crate::camera::MainCamera;
use crate::crafting::Station;
use crate::data::Db;
use crate::hud::text;
use crate::player::Player;
use crate::ui::Icons;
use crate::work::{Farm, FarmStage, Producer};

fn hex(s: &str) -> Color {
    Srgba::hex(s).map(Color::from).unwrap_or(Color::WHITE)
}

/// Panel for a facility entity.
#[derive(Component, Debug)]
struct WorkPanel(Entity);

#[derive(Component, Debug)]
struct WpIcon(Entity);

#[derive(Component, Debug)]
struct WpTitle(Entity);

#[derive(Component, Debug)]
struct WpSub(Entity);

#[derive(Component, Debug)]
struct WpFill(Entity);

struct Info {
    icon: Option<String>,
    title: String,
    sub: String,
    frac: f32,
    color: Color,
}

#[derive(Resource, Default)]
struct Frame(HashMap<Entity, Info>);

const RANGE_M: f32 = 15.0;

pub struct WorkHudPlugin;

impl Plugin for WorkHudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Frame>().add_systems(Update, (panels, parts).chain().after(crate::Sim::Last).after(crate::camera::follow));
    }
}

fn clock(secs: f32) -> String {
    let s = secs.max(0.0).ceil() as u32;
    format!("{:02}:{:02}", s / 60, s % 60)
}

fn eta(left: f32, rate: f32) -> String {
    if rate > 0.0 { format!("{} left", clock(left / rate)) } else { "waiting for a worker (or hold F)".into() }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn panels(
    mut commands: Commands,
    db: Res<Db>,
    scale: Res<UiScale>,
    ui: Res<crate::input::UiFocus>,
    rates: Res<crate::base::WorkContributions>,
    mut frame: ResMut<Frame>,
    cam: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    player: Query<&GlobalTransform, With<Player>>,
    things: Query<(Entity, &GlobalTransform, Option<&Station>, Option<&Producer>, Option<&Farm>), (With<Building>, Without<Construction>)>,
    workers: Query<&crate::base::WorkerTask>,
    mut q: Query<(Entity, &WorkPanel, &mut Node, &mut Visibility)>,
) {
    frame.0.clear();
    let (Ok((camera, ctf)), Ok(pt)) = (cam.single(), player.single()) else { return };
    let name = |id: &str| db.item(id).map_or(id.to_owned(), |i| i.name.clone());
    let mut have = Vec::new();
    for (pe, panel, mut node, mut vis) in &mut q {
        let Ok((e, tf, st, prod, farm)) = things.get(panel.0) else {
            commands.entity(pe).despawn();
            continue;
        };
        have.push(e);
        let rate = rates.rate(e);
        let on_it = workers.iter().filter(|w| w.job == Some(e)).count();
        let crew = if on_it > 0 { format!("   {on_it} worker{}", if on_it > 1 { "s" } else { "" }) } else { String::new() };
        let info = if let Some(st) = st {
            match st.queue.first() {
                Some(j) => {
                    let product = db.recipe(&j.recipe).map_or(j.recipe.clone(), |r| r.product.clone());
                    Some(Info { title: format!("{}  x{}", name(&product), j.remaining), sub: format!("{}{crew}", eta(j.need - j.work, rate)), frac: j.work / j.need.max(1.0), icon: Some(product), color: hex("FF9100") })
                }
                None if !st.output.is_empty() => {
                    let (i, n) = &st.output[0];
                    Some(Info { title: format!("Ready: {}  x{n}", name(i)), sub: "tap F to collect".into(), frac: 1.0, icon: Some(i.clone()), color: hex("1EFF9E") })
                }
                None => None,
            }
        } else if let Some(p) = prod {
            Some(Info { title: name(&p.product), sub: format!("{}{crew}", eta(p.need - p.work, rate)), frac: p.work / p.need.max(1.0), icon: Some(p.product.clone()), color: hex("FF9100") })
        } else if let Some(f) = farm {
            let (stage, sub) = match f.stage {
                FarmStage::Seeding => ("Planting", eta(f.need() - f.work, rate)),
                FarmStage::Watering => ("water", eta(f.need() - f.work, rate)),
                FarmStage::Growing => ("Growing", format!("{} left", clock(f.grow_time - f.grow))),
                FarmStage::Harvest => ("Harvest", eta(f.need() - f.work, rate)),
            };
            let frac = if f.stage == FarmStage::Growing { f.grow / f.grow_time.max(1.0) } else { f.work / f.need().max(1.0) };
            Some(Info { title: format!("{stage}: {} x{}", name(&f.crop), f.crop_num), sub: format!("{sub}{crew}"), frac, icon: Some(f.crop.clone()), color: if f.stage == FarmStage::Growing { hex("45F26B") } else { hex("FFC260") } })
        } else {
            None
        };
        let near = tf.translation().distance(pt.translation()) < RANGE_M;
        let at = camera.world_to_viewport(ctf, tf.translation() + Vec3::Y * 2.3).ok().map(|p| p / scale.0);
        match (info, at) {
            (Some(i), Some(p)) if near && !ui.0 => {
                *vis = Visibility::Inherited;
                node.left = px(p.x - 160.0);
                node.top = px(p.y - 70.0);
                frame.0.insert(pe, i);
            }
            _ => *vis = Visibility::Hidden,
        }
    }
    for (e, _, st, prod, farm) in &things {
        if have.contains(&e) || (st.is_none() && prod.is_none() && farm.is_none()) {
            continue;
        }
        let panel = commands.spawn((WorkPanel(e), Node { position_type: PositionType::Absolute, width: px(320.0), height: px(64.0), border: UiRect::top(px(2.0)), ..default() }, BackgroundColor(hex("11190ED0")), BorderColor::all(hex("FFC260")), Visibility::Hidden, GlobalZIndex(-1))).id();
        commands.entity(panel).with_children(|c| {
            c.spawn((WpIcon(panel), Node { position_type: PositionType::Absolute, left: px(6.0), top: px(6.0), width: px(48.0), height: px(48.0), ..default() }, ImageNode::default()));
            c.spawn((text("", 14.0), TextLayout::linebreak(LineBreak::NoWrap), WpTitle(panel), Node { position_type: PositionType::Absolute, left: px(62.0), top: px(4.0), ..default() }));
            c.spawn((text("", 12.0), TextLayout::linebreak(LineBreak::NoWrap), WpSub(panel), Node { position_type: PositionType::Absolute, left: px(62.0), top: px(24.0), ..default() }));
            c.spawn((Node { position_type: PositionType::Absolute, left: px(62.0), top: px(46.0), width: px(248.0), height: px(8.0), ..default() }, BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)))).with_children(|b| {
                b.spawn((WpFill(panel), Node { width: percent(0.0), height: percent(100.0), ..default() }, BackgroundColor(hex("FF9100"))));
            });
        });
    }
}

#[allow(clippy::type_complexity)]
fn parts(
    db: Res<Db>,
    icons: Res<Icons>,
    frame: Res<Frame>,
    mut imgs: Query<(&WpIcon, &mut ImageNode)>,
    mut titles: Query<(&WpTitle, &mut Text), Without<WpSub>>,
    mut subs: Query<(&WpSub, &mut Text), Without<WpTitle>>,
    mut fills: Query<(&WpFill, &mut Node, &mut BackgroundColor)>,
) {
    for (p, mut img) in &mut imgs {
        if let Some(h) = frame.0.get(&p.0).and_then(|i| i.icon.as_deref()).and_then(|id| icons.item(&db, id))
            && img.image != h
        {
            img.image = h;
        }
    }
    for (p, mut t) in &mut titles {
        if let Some(i) = frame.0.get(&p.0)
            && t.0 != i.title
        {
            t.0 = i.title.clone();
        }
    }
    for (p, mut t) in &mut subs {
        if let Some(i) = frame.0.get(&p.0)
            && t.0 != i.sub
        {
            t.0 = i.sub.clone();
        }
    }
    for (p, mut n, mut bg) in &mut fills {
        if let Some(i) = frame.0.get(&p.0) {
            n.width = percent(i.frac.clamp(0.0, 1.0) * 100.0);
            bg.0 = i.color;
        }
    }
}
