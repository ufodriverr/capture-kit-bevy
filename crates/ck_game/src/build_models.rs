//! Simple primitive models for build objects (boxes, cylinders, balls, cones), one per object family:
//! tables with legs, chests with lids, a campfire with logs and a flame, walls with window and door
//! holes, and so on. Placeholder art. The footprint (collider, ghost,
//! work reach) is the bounding box of the parts.

use bevy::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prim {
    Cube,
    Cyl,
    Ball,
    Cone,
    /// Four-sided cone (pyramid roofs).
    Pyramid,
}

#[derive(Debug, Clone)]
pub struct Part {
    pub prim: Prim,
    /// Full extents (a cylinder's diameter in x/z, height in y).
    pub size: Vec3,
    /// Centre, relative to the object's origin on the ground.
    pub at: Vec3,
    pub rot: Quat,
    pub color: Color,
    /// Emissive (flames, screens, water).
    pub glow: bool,
}

#[derive(Debug, Clone)]
pub struct Model {
    pub parts: Vec<Part>,
    /// Blocks movement (rugs, plots and the ranch don't).
    pub solid: bool,
    /// A warm point light (fires, torches), relative to the origin.
    pub light: Option<Vec3>,
}

impl Model {
    /// Bounding box size (x, y, z), with the origin at the bottom centre.
    pub fn size(&self) -> Vec3 {
        let mut half = Vec2::ZERO;
        let mut top: f32 = 0.05;
        for p in &self.parts {
            let m = Mat3::from_quat(p.rot);
            let h = p.size * 0.5;
            let ext = Vec3::new(m.x_axis.x.abs() * h.x + m.y_axis.x.abs() * h.y + m.z_axis.x.abs() * h.z, m.x_axis.y.abs() * h.x + m.y_axis.y.abs() * h.y + m.z_axis.y.abs() * h.z, m.x_axis.z.abs() * h.x + m.y_axis.z.abs() * h.y + m.z_axis.z.abs() * h.z);
            half = half.max(Vec2::new(p.at.x.abs() + ext.x, p.at.z.abs() + ext.z));
            top = top.max(p.at.y + ext.y);
        }
        Vec3::new((half.x * 2.0).max(0.1), top, (half.y * 2.0).max(0.1))
    }
}

fn c(r: f32, g: f32, b: f32) -> Color {
    Color::srgb(r, g, b)
}

const WOOD: (f32, f32, f32) = (0.55, 0.38, 0.22);
const WOOD_D: (f32, f32, f32) = (0.38, 0.25, 0.14);
const WOOD_L: (f32, f32, f32) = (0.74, 0.57, 0.36);
const STONE: (f32, f32, f32) = (0.56, 0.56, 0.58);
const STONE_D: (f32, f32, f32) = (0.4, 0.4, 0.43);
const METAL: (f32, f32, f32) = (0.5, 0.53, 0.57);
const IRON: (f32, f32, f32) = (0.28, 0.29, 0.32);
const GOLD: (f32, f32, f32) = (0.85, 0.68, 0.25);
const STRAW: (f32, f32, f32) = (0.86, 0.74, 0.42);
const CLOTH: (f32, f32, f32) = (0.88, 0.85, 0.76);
const LEAF: (f32, f32, f32) = (0.27, 0.56, 0.25);
const SOIL: (f32, f32, f32) = (0.36, 0.24, 0.15);
const FIRE: (f32, f32, f32) = (1.0, 0.55, 0.15);
const CYAN: (f32, f32, f32) = (0.3, 0.85, 1.0);
const RED: (f32, f32, f32) = (0.62, 0.16, 0.14);

/// Part list builder.
#[derive(Default)]
struct B {
    parts: Vec<Part>,
}

impl B {
    fn add(&mut self, prim: Prim, size: Vec3, at: Vec3, col: (f32, f32, f32)) -> &mut Self {
        self.parts.push(Part { prim, size, at, rot: Quat::IDENTITY, color: c(col.0, col.1, col.2), glow: false });
        self
    }
    fn bx(&mut self, sx: f32, sy: f32, sz: f32, x: f32, y: f32, z: f32, col: (f32, f32, f32)) -> &mut Self {
        self.add(Prim::Cube, Vec3::new(sx, sy, sz), Vec3::new(x, y, z), col)
    }
    fn cy(&mut self, d: f32, h: f32, x: f32, y: f32, z: f32, col: (f32, f32, f32)) -> &mut Self {
        self.add(Prim::Cyl, Vec3::new(d, h, d), Vec3::new(x, y, z), col)
    }
    fn ball(&mut self, d: f32, x: f32, y: f32, z: f32, col: (f32, f32, f32)) -> &mut Self {
        self.add(Prim::Ball, Vec3::splat(d), Vec3::new(x, y, z), col)
    }
    fn cone(&mut self, d: f32, h: f32, x: f32, y: f32, z: f32, col: (f32, f32, f32)) -> &mut Self {
        self.add(Prim::Cone, Vec3::new(d, h, d), Vec3::new(x, y, z), col)
    }
    /// Rotates the last part.
    fn rot(&mut self, q: Quat) -> &mut Self {
        if let Some(p) = self.parts.last_mut() {
            p.rot = q;
        }
        self
    }
    fn glow(&mut self) -> &mut Self {
        if let Some(p) = self.parts.last_mut() {
            p.glow = true;
        }
        self
    }
    /// A horizontal log/cylinder along x.
    fn log_x(&mut self, d: f32, len: f32, x: f32, y: f32, z: f32, col: (f32, f32, f32)) -> &mut Self {
        self.cy(d, len, x, y, z, col).rot(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2))
    }
    /// Four legs under a top of `w x d` at height `h`.
    fn legs(&mut self, w: f32, d: f32, h: f32, t: f32, col: (f32, f32, f32)) -> &mut Self {
        for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
            self.bx(t, h, t, sx * (w / 2.0 - t), h / 2.0, sz * (d / 2.0 - t), col);
        }
        self
    }
    /// A table: top + legs.
    fn table(&mut self, w: f32, d: f32, h: f32, col: (f32, f32, f32)) -> &mut Self {
        self.bx(w, 0.08, d, 0.0, h - 0.04, 0.0, col).legs(w, d, h - 0.08, 0.08, col)
    }
    /// A chair: seat, legs, backrest.
    fn chair(&mut self, w: f32, seat: f32, back: f32, col: (f32, f32, f32)) -> &mut Self {
        self.bx(w, 0.06, w, 0.0, seat, 0.0, col).legs(w, w, seat - 0.03, 0.06, col).bx(w, back, 0.06, 0.0, seat + back / 2.0, -w / 2.0 + 0.03, col)
    }
    /// An open shelf: sides, back and `n` boards.
    fn shelf(&mut self, w: f32, h: f32, d: f32, n: u32, col: (f32, f32, f32)) -> &mut Self {
        self.bx(0.06, h, d, -w / 2.0 + 0.03, h / 2.0, 0.0, col).bx(0.06, h, d, w / 2.0 - 0.03, h / 2.0, 0.0, col).bx(w, h, 0.03, 0.0, h / 2.0, -d / 2.0 + 0.015, WOOD_D);
        for i in 0..n {
            let y = 0.04 + i as f32 * (h - 0.08) / (n - 1).max(1) as f32;
            self.bx(w - 0.08, 0.04, d, 0.0, y, 0.0, col);
        }
        self
    }
    /// A chest: body, lid, bands.
    fn chest(&mut self, w: f32, h: f32, d: f32, body: (f32, f32, f32), band: (f32, f32, f32)) -> &mut Self {
        self.bx(w, h * 0.7, d, 0.0, h * 0.35, 0.0, body).bx(w + 0.04, h * 0.3, d + 0.04, 0.0, h * 0.85, 0.0, body);
        for x in [-w * 0.3, w * 0.3] {
            self.bx(0.06, h + 0.02, d + 0.06, x, h / 2.0, 0.0, band);
        }
        self.bx(0.12, 0.14, 0.04, 0.0, h * 0.7, d / 2.0 + 0.03, GOLD)
    }
    /// A plank wall `w x h` with posts at the sides.
    fn wall(&mut self, w: f32, h: f32, t: f32) -> &mut Self {
        self.bx(w, h, t, 0.0, h / 2.0, 0.0, WOOD).bx(0.2, h, t + 0.06, -w / 2.0 + 0.1, h / 2.0, 0.0, WOOD_D).bx(0.2, h, t + 0.06, w / 2.0 - 0.1, h / 2.0, 0.0, WOOD_D).bx(w, 0.16, t + 0.04, 0.0, h - 0.08, 0.0, WOOD_D)
    }
    /// A plant in a pot.
    fn potted(&mut self, pot: f32, h: f32) -> &mut Self {
        self.cy(pot, pot * 0.9, 0.0, pot * 0.45, 0.0, (0.66, 0.4, 0.28)).ball(h * 0.6, 0.0, pot * 0.9 + h * 0.3, 0.0, LEAF).ball(h * 0.4, pot * 0.3, pot * 0.9 + h * 0.5, 0.0, LEAF)
    }
    fn done(&mut self, solid: bool) -> Model {
        Model { parts: std::mem::take(&mut self.parts), solid, light: None }
    }
}

/// The model for a build object id (case-insensitive; unknown ids get a crate).
pub fn model(id: &str) -> Model {
    use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};
    let l = id.to_ascii_lowercase();
    let mut b = B::default();
    let lit = |mut m: Model, at: Vec3| {
        m.light = Some(at);
        m
    };
    match l.as_str() {
        // --- Production ---
        "workbench" => {
            b.table(1.6, 0.8, 0.9, WOOD).bx(1.4, 0.05, 0.6, 0.0, 0.3, 0.0, WOOD_D).bx(0.4, 0.05, 0.16, 0.3, 0.92, 0.1, METAL).bx(0.12, 0.2, 0.12, -0.6, 1.0, -0.25, WOOD_D);
            b.done(true)
        }
        "repair_bench" => {
            b.table(1.4, 0.8, 0.85, WOOD).bx(0.4, 0.22, 0.24, 0.1, 0.96, 0.0, IRON).cone(0.2, 0.3, 0.42, 0.96, 0.0, IRON).rot(Quat::from_rotation_z(-FRAC_PI_2)).bx(0.08, 0.3, 0.08, -0.45, 1.0, 0.1, WOOD_D);
            b.done(true)
        }
        "smelter" => {
            b.bx(1.5, 1.2, 1.5, 0.0, 0.6, 0.0, STONE).bx(1.6, 0.12, 1.6, 0.0, 1.2, 0.0, STONE_D).cy(0.6, 0.9, 0.0, 1.65, 0.0, STONE_D).bx(0.6, 0.45, 0.06, 0.0, 0.45, 0.75, FIRE).glow();
            lit(b.done(true), Vec3::new(0.0, 0.6, 1.0))
        }
        "stone_quarry" => {
            for (x, z, w, d) in [(0.0, -1.4, 3.0, 0.2), (0.0, 1.4, 3.0, 0.2), (-1.4, 0.0, 0.2, 3.0), (1.4, 0.0, 0.2, 3.0)] {
                b.bx(w, 0.7, d, x, 0.35, z, WOOD_D);
            }
            b.bx(2.6, 0.2, 2.6, 0.0, 0.1, 0.0, STONE_D).ball(0.6, -0.5, 0.3, 0.3, STONE).ball(0.5, 0.6, 0.28, -0.4, STONE).ball(0.4, 0.2, 0.25, 0.7, STONE);
            b.done(true)
        }
        "lumber_yard" => {
            for (i, y) in [0.15, 0.15, 0.15, 0.42, 0.42].iter().enumerate() {
                let z = if i < 3 { -0.6 + i as f32 * 0.32 } else { -0.44 + (i - 3) as f32 * 0.32 };
                b.log_x(0.3, 2.4, -0.3, *y, z, WOOD);
            }
            b.cy(0.6, 0.5, 1.1, 0.25, 0.5, WOOD_L).bx(0.05, 0.5, 0.05, 1.1, 0.7, 0.5, WOOD_D).bx(0.25, 0.12, 0.04, 1.2, 0.9, 0.5, METAL);
            b.done(true)
        }
        // --- Creatures ---
        "camp_core" => {
            // A stone cairn bound with rope, a carved post with a crossbar and pennants, a glowing crystal on top.
            b.cy(1.4, 0.35, 0.0, 0.17, 0.0, STONE_D).ball(0.6, -0.35, 0.5, 0.2, STONE).ball(0.55, 0.38, 0.48, -0.1, STONE).ball(0.5, 0.05, 0.5, -0.4, STONE_D).ball(0.45, 0.0, 0.85, 0.15, STONE);
            b.cy(0.95, 0.08, 0.0, 0.62, 0.0, STRAW).cy(0.22, 1.7, 0.0, 1.35, 0.0, WOOD).log_x(0.12, 1.1, 0.0, 1.85, 0.0, WOOD_D);
            b.bx(0.04, 0.4, 0.22, -0.48, 1.62, 0.0, GOLD).bx(0.04, 0.4, 0.22, 0.48, 1.62, 0.0, LEAF);
            b.cone(0.3, 0.45, 0.0, 2.42, 0.0, CYAN).glow().cone(0.3, 0.2, 0.0, 2.1, 0.0, CYAN).rot(Quat::from_rotation_x(std::f32::consts::PI)).glow();
            b.done(true)
        }
        "ranch" => {
            let (w, h) = (8.0, 1.1);
            for i in 0..=4 {
                let t = -w / 2.0 + w * i as f32 / 4.0;
                for (x, z) in [(t, -w / 2.0), (t, w / 2.0), (-w / 2.0, t), (w / 2.0, t)] {
                    b.bx(0.15, h, 0.15, x, h / 2.0, z, WOOD);
                }
            }
            for (x, z, r) in [(0.0, -w / 2.0, 0.0), (0.0, w / 2.0, 0.0), (-w / 2.0, 0.0, FRAC_PI_2), (w / 2.0, 0.0, FRAC_PI_2)] {
                b.bx(w, 0.1, 0.08, x, h * 0.7, z, WOOD_L).rot(Quat::from_rotation_y(r));
                b.bx(w, 0.1, 0.08, x, h * 0.35, z, WOOD_L).rot(Quat::from_rotation_y(r));
            }
            b.bx(1.0, 0.3, 0.5, 2.5, 0.15, 2.5, STRAW);
            b.done(false)
        }
        "job_board" => {
            b.bx(1.2, 0.3, 1.2, 0.0, 0.15, 0.0, WOOD).bx(0.12, 1.8, 0.12, 0.0, 1.2, 0.0, WOOD_D).cone(0.4, 0.5, 0.0, 2.1, 0.2, METAL).rot(Quat::from_rotation_x(FRAC_PI_2)).bx(0.8, 0.5, 0.05, 0.0, 1.0, 0.1, (0.9, 0.85, 0.5));
            b.done(true)
        }
        // --- Storage ---
        "storage_chest" => {
            b.chest(1.0, 0.7, 0.7, WOOD, WOOD_D);
            b.done(true)
        }
        "large_chest" => {
            b.chest(1.1, 0.8, 0.75, METAL, IRON);
            b.done(true)
        }
        "feed_box" => {
            b.bx(1.4, 0.5, 1.0, 0.0, 0.25, 0.0, WOOD).bx(1.3, 0.1, 0.9, 0.0, 0.48, 0.0, STRAW);
            for (x, z, col) in [(-0.4, -0.2, (0.85, 0.2, 0.2)), (-0.1, 0.15, (0.4, 0.2, 0.6)), (0.25, -0.1, (0.85, 0.2, 0.2)), (0.45, 0.2, (0.9, 0.7, 0.2))] {
                b.ball(0.22, x, 0.58, z, col);
            }
            b.done(true)
        }
        "barrel" => {
            b.cy(0.7, 0.95, 0.0, 0.475, 0.0, WOOD).cy(0.74, 0.06, 0.0, 0.2, 0.0, IRON).cy(0.74, 0.06, 0.0, 0.75, 0.0, IRON);
            b.done(true)
        }
        "crate" => {
            b.bx(0.8, 0.8, 0.8, 0.0, 0.4, 0.0, WOOD_L).bx(0.84, 0.08, 0.84, 0.0, 0.04, 0.0, WOOD_D).bx(0.84, 0.08, 0.84, 0.0, 0.76, 0.0, WOOD_D).bx(0.08, 0.8, 0.84, 0.0, 0.4, 0.0, WOOD_D);
            b.done(true)
        }
        "shelf" => {
            b.shelf(1.4, 1.8, 0.5, 4, WOOD).bx(0.3, 0.25, 0.3, -0.4, 0.68, 0.0, WOOD_L).cy(0.2, 0.25, 0.3, 1.25, 0.0, (0.7, 0.5, 0.3));
            b.done(true)
        }
        // --- Food ---
        "campfire" => {
            for k in 0..8 {
                let a = k as f32 / 8.0 * std::f32::consts::TAU;
                b.ball(0.24, a.cos() * 0.38, 0.1, a.sin() * 0.38, STONE);
            }
            b.log_x(0.12, 0.7, 0.0, 0.12, 0.0, WOOD_D).rot(Quat::from_rotation_z(FRAC_PI_2) * Quat::from_rotation_x(0.6));
            b.log_x(0.12, 0.7, 0.0, 0.12, 0.0, WOOD_D).rot(Quat::from_rotation_y(FRAC_PI_2) * Quat::from_rotation_z(FRAC_PI_2));
            b.cone(0.4, 0.55, 0.0, 0.42, 0.0, FIRE).glow().cone(0.22, 0.35, 0.06, 0.38, 0.05, (1.0, 0.85, 0.3)).glow();
            lit(b.done(true), Vec3::new(0.0, 0.7, 0.0))
        }
        "berry_patch" => {
            b.bx(3.2, 0.15, 3.2, 0.0, 0.075, 0.0, SOIL);
            for (x, z, w, d) in [(0.0, -1.55, 3.2, 0.12), (0.0, 1.55, 3.2, 0.12), (-1.55, 0.0, 0.12, 3.2), (1.55, 0.0, 0.12, 3.2)] {
                b.bx(w, 0.25, d, x, 0.125, z, WOOD);
            }
            for z in [-0.9, 0.0, 0.9] {
                b.bx(2.8, 0.06, 0.3, 0.0, 0.18, z, (0.28, 0.18, 0.1));
            }
            b.done(false)
        }
        // --- Infrastructure ---
        "bed" => {
            b.bx(1.0, 0.25, 2.0, 0.0, 0.125, 0.0, WOOD).bx(0.9, 0.15, 1.85, 0.0, 0.32, 0.05, STRAW).bx(0.6, 0.12, 0.35, 0.0, 0.44, -0.72, CLOTH).bx(1.0, 0.6, 0.08, 0.0, 0.3, -0.98, WOOD_D);
            b.done(true)
        }
        "creature_bed" => {
            b.cy(1.2, 0.2, 0.0, 0.1, 0.0, STRAW).add(Prim::Cyl, Vec3::new(1.25, 0.18, 1.25), Vec3::new(0.0, 0.25, 0.0), (0.76, 0.62, 0.32));
            b.cy(0.9, 0.08, 0.0, 0.36, 0.0, (0.92, 0.82, 0.5));
            // A nest a creature sleeps in, not an obstacle.
            b.done(false)
        }
        // --- Light ---
        "standing_torch" => {
            b.cy(0.12, 1.5, 0.0, 0.75, 0.0, WOOD_D).cy(0.22, 0.2, 0.0, 1.55, 0.0, IRON).cone(0.2, 0.35, 0.0, 1.8, 0.0, FIRE).glow();
            lit(b.done(true), Vec3::new(0.0, 1.9, 0.0))
        }
        "wall_torch" => {
            b.bx(0.12, 1.3, 0.12, 0.0, 0.65, 0.0, WOOD_D).bx(0.08, 0.08, 0.3, 0.0, 1.35, 0.12, IRON).cone(0.16, 0.3, 0.0, 1.5, 0.27, FIRE).glow();
            lit(b.done(true), Vec3::new(0.0, 1.6, 0.3))
        }
        "fireplace" => {
            let col = if l.ends_with("01") { (0.62, 0.3, 0.22) } else { STONE };
            b.bx(1.6, 1.2, 0.7, 0.0, 0.6, -0.1, col).bx(0.8, 0.6, 0.1, 0.0, 0.4, 0.26, (0.1, 0.08, 0.07)).cone(0.45, 0.4, 0.0, 0.3, 0.1, FIRE).glow().bx(1.8, 0.12, 0.8, 0.0, 1.26, -0.1, col).bx(0.6, 1.0, 0.5, 0.0, 1.8, -0.2, col);
            lit(b.done(true), Vec3::new(0.0, 0.5, 0.6))
        }
        // --- Defense ---
        "spiked_wall" => {
            for k in 0..9 {
                let x = -1.8 + k as f32 * 0.45;
                b.cy(0.42, 2.6, x, 1.3, 0.0, WOOD).cone(0.42, 0.45, x, 2.82, 0.0, WOOD_L);
            }
            b.bx(4.0, 0.15, 0.5, 0.0, 1.8, 0.0, WOOD_D).bx(4.0, 0.15, 0.5, 0.0, 0.7, 0.0, WOOD_D);
            b.done(true)
        }
        "wood_gate" => {
            b.bx(0.4, 3.2, 0.4, -2.0, 1.6, 0.0, WOOD_D).bx(0.4, 3.2, 0.4, 2.0, 1.6, 0.0, WOOD_D).bx(4.4, 0.3, 0.4, 0.0, 3.25, 0.0, WOOD_D).bx(3.6, 2.6, 0.15, 0.0, 1.4, 0.0, WOOD).bx(3.6, 0.15, 0.2, 0.0, 2.2, 0.0, WOOD_D).bx(3.6, 0.15, 0.2, 0.0, 0.7, 0.0, WOOD_D);
            b.done(true)
        }
        "snare" => {
            b.cy(0.6, 0.04, 0.0, 0.02, 0.0, IRON);
            for k in 0..8 {
                let a = k as f32 / 8.0 * std::f32::consts::TAU;
                b.cone(0.06, 0.14, a.cos() * 0.26, 0.1, a.sin() * 0.26, METAL);
            }
            b.done(false)
        }
        // --- Foundations ---
        "wood_foundation" => {
            b.bx(4.0, 0.4, 4.0, 0.0, 0.2, 0.0, WOOD_L);
            for k in 0..4 {
                b.bx(0.04, 0.01, 4.0, -1.5 + k as f32, 0.405, 0.0, WOOD_D);
            }
            b.done(true)
        }
        "wood_triangle_foundation" => {
            for k in 0..4 {
                let w = 4.0 * (1.0 - k as f32 / 4.0);
                b.bx(w, 0.4, 1.0, 0.0, 0.2, -1.5 + k as f32, WOOD_L);
            }
            b.done(true)
        }
        "wood_wall" => {
            b.wall(4.0, 3.0, 0.25);
            b.done(true)
        }
        "wood_window_wall" => {
            b.bx(4.0, 1.0, 0.25, 0.0, 0.5, 0.0, WOOD).bx(4.0, 0.8, 0.25, 0.0, 2.6, 0.0, WOOD).bx(1.2, 1.2, 0.25, -1.4, 1.6, 0.0, WOOD).bx(1.2, 1.2, 0.25, 1.4, 1.6, 0.0, WOOD).bx(1.7, 0.1, 0.3, 0.0, 1.0, 0.0, WOOD_D).bx(0.08, 1.2, 0.3, 0.0, 1.6, 0.0, WOOD_D);
            b.done(true)
        }
        "wood_door" => {
            let (dw, dh) = (1.2, 2.2);
            let side = (4.0 - dw) / 2.0;
            b.bx(side, 3.0, 0.25, -2.0 + side / 2.0, 1.5, 0.0, WOOD).bx(side, 3.0, 0.25, 2.0 - side / 2.0, 1.5, 0.0, WOOD).bx(dw, 3.0 - dh, 0.25, 0.0, dh + (3.0 - dh) / 2.0, 0.0, WOOD).bx(dw + 0.2, 0.15, 0.32, 0.0, dh, 0.0, WOOD_D);
            b.bx(0.08, dh, dw * 0.9, -dw / 2.0, dh / 2.0, dw * 0.45, WOOD_D);
            b.done(false)
        }
        "wood_triangle_wall" => {
            for k in 0..4 {
                let w = 4.0 * (1.0 - k as f32 / 4.0);
                b.bx(w, 0.5, 0.25, 0.0, 0.25 + k as f32 * 0.5, 0.0, WOOD);
            }
            if l.ends_with("reverse") {
                for p in &mut b.parts {
                    p.at.y = 2.0 - p.at.y;
                }
            }
            b.done(true)
        }
        "wood_roof" => {
            b.bx(4.2, 0.2, 4.2, 0.0, 0.1, 0.0, WOOD_D);
            b.done(true)
        }
        "wood_slanted_roof" => {
            let tilt = 0.52;
            b.bx(4.2, 0.18, 4.6, 0.0, 1.15, 0.0, (0.55, 0.3, 0.22)).rot(Quat::from_rotation_x(tilt));
            if l != "wood_slantedroof" {
                for p in &mut b.parts {
                    p.rot = Quat::from_rotation_y(FRAC_PI_4) * p.rot;
                }
            }
            b.done(true)
        }
        "wood_stairs" => {
            for k in 0..6 {
                b.bx(2.0, 0.5, 0.6, 0.0, 0.25 + k as f32 * 0.5, -1.5 + k as f32 * 0.6, WOOD_L);
            }
            b.done(true)
        }
        "wood_pillar" => {
            b.bx(0.4, 3.0, 0.4, 0.0, 1.5, 0.0, WOOD_D).bx(0.5, 0.15, 0.5, 0.0, 0.075, 0.0, WOOD).bx(0.5, 0.15, 0.5, 0.0, 2.93, 0.0, WOOD);
            b.done(true)
        }
        "wood_fence" => {
            b.bx(0.12, 1.1, 0.12, -1.9, 0.55, 0.0, WOOD_D).bx(0.12, 1.1, 0.12, 1.9, 0.55, 0.0, WOOD_D).bx(0.12, 1.1, 0.12, 0.0, 0.55, 0.0, WOOD_D).bx(4.0, 0.1, 0.06, 0.0, 0.8, 0.0, WOOD).bx(4.0, 0.1, 0.06, 0.0, 0.4, 0.0, WOOD);
            b.done(true)
        }
        "wood_ladder" => {
            b.bx(0.08, 3.0, 0.08, -0.3, 1.5, 0.0, WOOD_D).bx(0.08, 3.0, 0.08, 0.3, 1.5, 0.0, WOOD_D);
            for k in 0..8 {
                b.bx(0.6, 0.05, 0.06, 0.0, 0.3 + k as f32 * 0.36, 0.0, WOOD);
            }
            b.done(false)
        }
        // --- Furniture / other ---
        "table" => {
            b.table(1.2, 1.2, 0.78, WOOD);
            if l.ends_with("none") {
                b.bx(1.0, 0.01, 1.0, 0.0, 0.785, 0.0, CLOTH);
            }
            b.done(true)
        }
        "round_table" => {
            let col = if l.ends_with("stone") { WOOD_D } else { WOOD };
            b.cy(1.2, 0.08, 0.0, 0.76, 0.0, col).cy(0.12, 0.72, 0.0, 0.36, 0.0, col).cy(0.6, 0.06, 0.0, 0.03, 0.0, col);
            if l.ends_with("none") {
                b.cy(0.5, 0.01, 0.0, 0.805, 0.0, CLOTH);
            }
            b.done(true)
        }
        "bench" => {
            b.bx(1.6, 0.07, 0.42, 0.0, 0.45, 0.0, WOOD).legs(1.6, 0.42, 0.42, 0.07, WOOD_D).bx(1.6, 0.35, 0.06, 0.0, 0.7, -0.19, WOOD);
            b.done(true)
        }
        "stool" => {
            b.cy(0.42, 0.06, 0.0, 0.45, 0.0, WOOD).legs(0.36, 0.36, 0.42, 0.05, WOOD_D);
            b.done(true)
        }
        "chair" => {
            b.chair(0.48, 0.46, 0.5, WOOD);
            b.done(true)
        }
        "counter" => {
            b.bx(2.0, 1.0, 0.6, 0.0, 0.5, 0.0, WOOD).bx(2.1, 0.08, 0.7, 0.0, 1.04, 0.0, WOOD_L).bx(1.9, 0.03, 0.03, 0.0, 0.6, 0.31, WOOD_D);
            b.done(true)
        }
        "signpost" => {
            b.bx(0.1, 1.4, 0.1, 0.0, 0.7, 0.0, WOOD_D).bx(1.0, 0.6, 0.06, 0.0, 1.35, 0.06, WOOD_L);
            b.done(true)
        }
        "scarecrow" => {
            b.cy(0.12, 1.8, 0.0, 0.9, 0.0, WOOD_D).log_x(0.08, 1.2, 0.0, 1.35, 0.0, WOOD_D).cy(0.45, 0.6, 0.0, 1.2, 0.0, STRAW).ball(0.36, 0.0, 1.75, 0.0, (0.8, 0.7, 0.5)).bx(0.5, 0.06, 0.05, 0.0, 1.0, 0.24, RED);
            b.done(true)
        }
        "banner" => {
            b.cy(0.08, 2.8, 0.0, 1.4, 0.0, IRON).cy(0.4, 0.1, 0.0, 0.05, 0.0, IRON);
            b.bx(0.7, 1.4, 0.03, 0.0, 1.9, 0.06, (0.25, 0.1, 0.12)).bx(0.8, 0.06, 0.06, 0.0, 2.62, 0.06, IRON);
            b.done(true)
        }
        "potted_plant" => {
            b.potted(0.4, 0.7);
            b.done(true)
        }
        "rug" => {
            let col = (0.62, 0.18, 0.16);
            b.bx(2.0, 0.02, 1.4, 0.0, 0.01, 0.0, col).bx(1.6, 0.022, 1.05, 0.0, 0.012, 0.0, (col.0 * 0.7, col.1 * 0.7, col.2 * 0.7));
            b.done(false)
        }
        _ => {
            // Unknown: a wooden crate.
            b.bx(1.2, 1.0, 1.2, 0.0, 0.5, 0.0, WOOD_L).bx(1.24, 0.1, 1.24, 0.0, 0.95, 0.0, WOOD_D).bx(1.24, 0.1, 1.24, 0.0, 0.05, 0.0, WOOD_D);
            b.done(true)
        }
    }
}

const CUBE: Handle<Mesh> = bevy::asset::uuid_handle!("6a1f0c2e-6a52-4c39-9b55-0d8f7b0a1001");
const CYL: Handle<Mesh> = bevy::asset::uuid_handle!("6a1f0c2e-6a52-4c39-9b55-0d8f7b0a1002");
const BALL: Handle<Mesh> = bevy::asset::uuid_handle!("6a1f0c2e-6a52-4c39-9b55-0d8f7b0a1003");
const CONE: Handle<Mesh> = bevy::asset::uuid_handle!("6a1f0c2e-6a52-4c39-9b55-0d8f7b0a1004");
const PYRAMID: Handle<Mesh> = bevy::asset::uuid_handle!("6a1f0c2e-6a52-4c39-9b55-0d8f7b0a1005");

/// The shared unit mesh for a primitive (1 m cube, cylinder, ball, cone, pyramid), made on first use.
pub fn prim_mesh(meshes: &mut Assets<Mesh>, p: Prim) -> Handle<Mesh> {
    let h = match p {
        Prim::Cube => CUBE,
        Prim::Cyl => CYL,
        Prim::Ball => BALL,
        Prim::Cone => CONE,
        Prim::Pyramid => PYRAMID,
    };
    if !meshes.contains(&h) {
        let mesh: Mesh = match p {
            Prim::Cube => Cuboid::new(1.0, 1.0, 1.0).into(),
            Prim::Cyl => Cylinder::new(0.5, 1.0).mesh().resolution(16).into(),
            Prim::Ball => Sphere::new(0.5).mesh().ico(2).unwrap_or_else(|_| Sphere::new(0.5).into()),
            Prim::Cone => Cone { radius: 0.5, height: 1.0 }.mesh().resolution(16).into(),
            Prim::Pyramid => Cone { radius: 0.5, height: 1.0 }.mesh().resolution(4).into(),
        };
        let _ = meshes.insert(&h, mesh);
    }
    h
}

/// The part's local transform (unit mesh scaled to its size).
pub fn part_transform(p: &Part) -> Transform {
    let rot = if p.prim == Prim::Pyramid { p.rot * Quat::from_rotation_y(std::f32::consts::FRAC_PI_4) } else { p.rot };
    Transform::from_translation(p.at).with_rotation(rot).with_scale(p.size)
}

#[cfg(test)]
mod tests {
    use super::model;

    #[test]
    fn models_have_parts_and_sane_bounds() {
        for id in ["workbench", "campfire", "wood_wall", "table", "chair", "ranch", "berry_patch", "camp_core", "SomethingNew"] {
            let m = model(id);
            let s = m.size();
            assert!(!m.parts.is_empty(), "{id}");
            assert!(s.x > 0.2 && s.y > 0.0 && s.z > 0.2 && s.max_element() < 10.0, "{id} {s}");
        }
        let wall = model("wood_wall").size();
        assert!((wall.x - 4.0).abs() < 0.1 && (wall.y - 3.0).abs() < 0.05, "{wall}");
        assert!(!model("ranch").solid && model("storage_chest").solid);
    }
}
