//! Kinematic capsule movement against the static world (terrain, trees, rocks, buildings): collide and slide
//! with avian shape casts, step-up, ground snapping. Characters (player, creatures) have no physics bodies; the
//! world is static, so a cast is all they need.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::world::{Layer, Terrain};

pub const SKIN: f32 = 0.02;
/// Default walkable floor: normal y at least cos 50 degrees = 0.6428 (creatures use 45 degrees = 0.7071).
pub const WALKABLE_COS: f32 = 0.6428;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Capsule {
    pub radius: f32,
    /// Total half height including the hemispheres.
    pub half_height: f32,
}

impl Capsule {
    pub fn collider(&self) -> Collider {
        Collider::capsule(self.radius, (2.0 * (self.half_height - self.radius)).max(0.01))
    }
}

/// A kinematic character. `feet` is the bottom of the capsule.
#[derive(Component, Debug, Clone)]
pub struct Mover {
    pub feet: Vec3,
    pub vel: Vec3,
    pub grounded: bool,
    pub ground_normal: Vec3,
    pub capsule: Capsule,
    /// Facing (radians, 0 = -Z, positive = turning left / counter-clockwise seen from above).
    pub yaw: f32,
    /// Max step-up height.
    pub step_height: f32,
    /// What the last move bumped into (normals), for climbing and AI.
    pub contacts: Vec<Vec3>,
    /// Floors with a normal's y at least this are walkable.
    pub walkable: f32,
}

impl Mover {
    pub fn new(feet: Vec3, capsule: Capsule, step_height: f32) -> Self {
        Self { feet, vel: Vec3::ZERO, grounded: false, ground_normal: Vec3::Y, capsule, yaw: 0.0, step_height, contacts: Vec::new(), walkable: WALKABLE_COS }
    }

    pub fn center(&self) -> Vec3 {
        self.feet + Vec3::Y * self.capsule.half_height
    }

    pub fn forward(&self) -> Vec3 {
        yaw_forward(self.yaw)
    }
}

pub fn yaw_forward(yaw: f32) -> Vec3 {
    Vec3::new(-yaw.sin(), 0.0, -yaw.cos())
}

pub fn yaw_of(dir: Vec3) -> f32 {
    (-dir.x).atan2(-dir.z)
}

/// Turns `from` toward `to` by at most `max_step` radians.
pub fn turn_toward(from: f32, to: f32, max_step: f32) -> f32 {
    let mut d = (to - from) % std::f32::consts::TAU;
    if d > std::f32::consts::PI {
        d -= std::f32::consts::TAU;
    } else if d < -std::f32::consts::PI {
        d += std::f32::consts::TAU;
    }
    from + d.clamp(-max_step, max_step)
}

pub fn world_filter(exclude: &[Entity]) -> SpatialQueryFilter {
    SpatialQueryFilter::from_mask([Layer::Terrain, Layer::Solid]).with_excluded_entities(exclude.iter().copied())
}

fn cast(sq: &SpatialQuery, shape: &Collider, from: Vec3, motion: Vec3, filter: &SpatialQueryFilter) -> Option<ShapeHitData> {
    let len = motion.length();
    let dir = Dir3::new(motion / len).ok()?;
    let cfg = ShapeCastConfig { max_distance: len + SKIN, target_distance: 0.0, compute_contact_on_penetration: true, ignore_origin_penetration: true };
    sq.cast_shape(shape, from, Quat::IDENTITY, dir, &cfg, filter)
}

/// Casts the capsule from `center` along the unit `dir`: distance and surface normal of the first hit within
/// `reach`.
pub fn cast_dir(sq: &SpatialQuery, shape: &Collider, center: Vec3, dir: Vec3, reach: f32, filter: &SpatialQueryFilter) -> Option<(f32, Vec3)> {
    cast(sq, shape, center, dir * reach, filter).map(|h| (h.distance, h.normal1.normalize_or_zero()))
}

/// Moves the capsule centre by `motion`, sliding along what it hits. Returns the new centre and the hit normals.
pub fn slide(sq: &SpatialQuery, shape: &Collider, mut center: Vec3, mut motion: Vec3, filter: &SpatialQueryFilter) -> (Vec3, Vec<Vec3>) {
    let mut normals = Vec::new();
    for _ in 0..4 {
        let len = motion.length();
        if len < 1e-5 {
            break;
        }
        match cast(sq, shape, center, motion, filter) {
            None => {
                center += motion;
                break;
            }
            Some(hit) => {
                let dir = motion / len;
                let travel = (hit.distance - SKIN).max(0.0);
                center += dir * travel;
                let n = hit.normal1.normalize_or_zero();
                normals.push(n);
                // Already touching (inside the skin): step back off the surface, or every cast along it
                // would hit at 0 again (a capsule wedged where two terrain triangles meet never moved).
                if travel < SKIN * 0.5 {
                    center += n * SKIN * 0.5;
                }
                let rest = motion - dir * travel;
                motion = rest - n * rest.dot(n).min(0.0);
                // Two planes (a crease or corner): slide along their shared edge, or stop when that would
                // still push into the first one.
                if normals.len() >= 2 {
                    let prev = normals[normals.len() - 2];
                    if motion.dot(prev) < 0.0 {
                        let crease = prev.cross(n).normalize_or_zero();
                        motion = crease * motion.dot(crease);
                    }
                    if prev.dot(n) < -0.2 {
                        motion = Vec3::ZERO;
                    }
                }
            }
        }
    }
    (center, normals)
}

/// One movement step: horizontal slide with step-up, then vertical, then ground snap. `snap` is how far down
/// to look for the floor when grounded (0 when jumping). Updates `m.feet`, `m.grounded`, `m.ground_normal`,
/// `m.contacts`, and zeroes blocked velocity.
/// Pushes the capsule out of anything it overlaps (after a teleport, a spawned building, a moved collider).
/// The casts ignore origin penetration, so without this an overlapping capsule could never move again.
fn depenetrate(sq: &SpatialQuery, shape: &Collider, mut center: Vec3, filter: &SpatialQueryFilter) -> Vec3 {
    let cfg = ShapeCastConfig { max_distance: 0.0, target_distance: 0.0, compute_contact_on_penetration: true, ignore_origin_penetration: false };
    for _ in 0..4 {
        let Some(hit) = sq.cast_shape(shape, center, Quat::IDENTITY, Dir3::NEG_Y, &cfg, filter) else { break };
        if hit.distance > 0.0 {
            break;
        }
        let depth = (hit.point1 - hit.point2).length();
        if depth < 1e-4 {
            break;
        }
        center += hit.normal1 * (depth + SKIN);
    }
    center
}

pub fn move_character(sq: &SpatialQuery, terrain: &Terrain, m: &mut Mover, dt: f32, snap: f32, filter: &SpatialQueryFilter) {
    let shape = m.capsule.collider();
    let mut center = depenetrate(sq, &shape, m.center(), filter);
    m.contacts.clear();
    let horiz = Vec3::new(m.vel.x, 0.0, m.vel.z) * dt;
    // On walkable ground, move along the slope so walking downhill doesn't bounce.
    let horiz = if m.grounded && m.ground_normal.y >= m.walkable && m.vel.y <= 0.0 { horiz - m.ground_normal * horiz.dot(m.ground_normal) } else { horiz };
    if horiz.length_squared() > 1e-10 {
        let (c1, n1) = slide(sq, &shape, center, horiz, filter);
        let blocked = n1.iter().any(|n| n.y < m.walkable && n.y > -0.3);
        let mut chosen = (c1, n1);
        if blocked && m.grounded && m.step_height > 0.0 {
            // Step up: lift, move, drop back down.
            let (up, _) = slide(sq, &shape, center, Vec3::Y * m.step_height, filter);
            let (fwd, n2) = slide(sq, &shape, up, horiz, filter);
            let lifted = up.y - center.y;
            if let Some(hit) = cast(sq, &shape, fwd, Vec3::NEG_Y * (lifted + SKIN * 2.0), filter)
                && hit.normal1.y >= m.walkable
            {
                let down = fwd - Vec3::Y * (hit.distance - SKIN).max(0.0);
                let gain = (down - center) * Vec3::new(1.0, 0.0, 1.0);
                if gain.length() > (chosen.0 - center).with_y(0.0).length() + 0.01 {
                    chosen = (down, n2);
                }
            }
        }
        center = chosen.0;
        m.contacts.extend(chosen.1.iter().copied());
        for n in &chosen.1 {
            if n.y < m.walkable {
                let flat = Vec3::new(n.x, 0.0, n.z).normalize_or_zero();
                let into = m.vel.dot(flat);
                if into < 0.0 {
                    m.vel -= flat * into;
                }
            }
        }
    }
    // Vertical.
    let vert = Vec3::Y * m.vel.y * dt;
    let mut landed_normal = None;
    let lands = if vert.y < 0.0 { cast(sq, &shape, center, vert, filter).filter(|h| h.normal1.y >= m.walkable) } else { None };
    if let Some(hit) = lands {
        center.y -= (hit.distance - SKIN).max(0.0);
        let n = hit.normal1.normalize_or_zero();
        landed_normal = Some(n);
        m.contacts.push(n);
    } else if vert.y.abs() > 1e-6 {
        let (c2, n2) = slide(sq, &shape, center, vert, filter);
        center = c2;
        for n in n2 {
            if m.vel.y < 0.0 && n.y >= m.walkable {
                landed_normal = Some(n);
            }
            if m.vel.y > 0.0 && n.y < -0.3 {
                m.vel.y = 0.0; // ceiling
            }
            m.contacts.push(n);
        }
    }
    // Ground probe.
    let probe = snap.max(SKIN * 3.0);
    let ground = if m.vel.y <= 0.0 { cast(sq, &shape, center, Vec3::NEG_Y * probe, filter).filter(|h| h.normal1.y >= m.walkable) } else { None };
    match (ground, landed_normal) {
        (Some(h), _) => {
            center.y -= (h.distance - SKIN).max(0.0);
            m.grounded = true;
            m.ground_normal = h.normal1.normalize_or_zero();
        }
        (None, Some(n)) => {
            m.grounded = true;
            m.ground_normal = n;
        }
        _ => {
            m.grounded = false;
            m.ground_normal = Vec3::Y;
        }
    }
    if m.grounded && m.vel.y < 0.0 {
        m.vel.y = 0.0;
    }
    // Safety net: never end below the terrain (very fast falls, spawning inside things).
    let floor = terrain.height(center.x, center.z);
    if center.y - m.capsule.half_height < floor - 0.25 {
        center.y = floor + m.capsule.half_height;
        m.vel.y = m.vel.y.max(0.0);
        m.grounded = true;
    }
    m.feet = center - Vec3::Y * m.capsule.half_height;
}
