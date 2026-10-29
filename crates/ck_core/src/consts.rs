//! Engine constants that aren't content (feel and timing). Units: metres, seconds.
//! Content values (stats, costs, rates) live in content/*.ron instead.

/// Stamina regained per second when not spending.
pub const STAMINA_REGEN_PER_S: f32 = 30.0;
/// Delay after spending before regen starts.
pub const STAMINA_REGEN_DELAY_S: f32 = 1.0;
/// After running dry, sprint and jump stay locked until stamina is back to this fraction.
pub const STAMINA_EXHAUST_UNLOCK_FRACTION: f32 = 0.3;

/// Unarmed attack value.
pub const FIST_ATTACK: f32 = 20.0;
/// A resource node drops one batch each time this fraction of its max HP is removed, plus a final batch.
pub const NODE_DROP_STEP: f32 = 0.1;
/// Resource respawn time after it was broken or picked.
pub const NODE_RESPAWN_S: f32 = 180.0;
/// Melee swing time and when in it the hit lands.
pub const SWING_TIME_S: f32 = 0.55;
pub const SWING_HIT_AT_S: f32 = 0.22;
/// Melee reach in front of the chest and hit radius.
pub const MELEE_REACH_M: f32 = 1.2;
pub const MELEE_RADIUS_M: f32 = 0.9;
/// Arrow launch speed (m/s) and reload time.
pub const ARROW_SPEED: f32 = 55.0;
pub const BOW_RELOAD_S: f32 = 0.8;
/// Durability lost per hit with a tool or weapon.
pub const DURABILITY_PER_HIT: f32 = 1.0;
/// Technology points at the start.
pub const START_TECH_POINTS: u32 = 0;
/// Camp creatures go to eat below this fraction of a full stomach.
pub const EAT_BELOW: f32 = 0.5;
/// Creature stomach decrease per second (x the species `stomach_rate`).
pub const STOMACH_PER_S: f32 = 0.05;
/// A ranching creature makes one loot roll every RANCH_WORK_PER_DROP / its ranch speed seconds.
pub const RANCH_WORK_PER_DROP: f32 = 1200.0;
/// Healing in the Camp Core's storage, fraction of max HP per second.
pub const STORAGE_HEAL_PER_S: f32 = 0.02;
/// Steepest ground a building can stand on (normal y).
pub const BUILD_MIN_NORMAL_Y: f32 = 0.75;
/// Dismantling: hold time and reach.
pub const DISMANTLE_HOLD_S: f32 = 1.0;
pub const DISMANTLE_RANGE_M: f32 = 12.0;
/// Dropped items within this distance fly to the player and are picked up.
pub const AUTO_PICKUP_M: f32 = 2.5;
pub const PICKUP_PULL_SPEED: f32 = 9.0;
/// The player's bed works from SLEEP_FROM_HOUR until morning and wakes the player at WAKE_HOUR.
pub const SLEEP_FROM_HOUR: f32 = 18.5;
pub const WAKE_HOUR: f32 = 6.0;
