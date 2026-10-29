//! Game content: `content/*.ron` loaded into typed tables (docs/CONTENT.md). Ids are matched
//! case-insensitively. [`GameData::validate`] reports every reference to something that doesn't exist.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::stats::Element;

fn one() -> u32 {
    1
}
fn one_f() -> f32 {
    1.0
}
fn far() -> f32 {
    100.0
}

#[derive(Debug, Clone, Deserialize)]
pub struct Item {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub desc: String,
    /// Material, Food, Weapon, Armor, Glider, Orb, Ammo.
    pub kind: String,
    /// Weapon: Melee, Axe, Pickaxe, Spear, Bow, Torch, Shield; Armor: Body, Head; Ammo: Arrow.
    #[serde(default)]
    pub sub: String,
    #[serde(default = "one")]
    pub max_stack: u32,
    #[serde(default)]
    pub weight: f32,
    #[serde(default)]
    pub durability: Option<f32>,
    #[serde(default)]
    pub attack: f32,
    #[serde(default)]
    pub magazine: u32,
    /// Food: hunger restored.
    #[serde(default)]
    pub food: f32,
    /// Orb: capture strength.
    #[serde(default)]
    pub orb_level: f32,
    /// Ammo: the element and status (rules.ron `statuses`) its hit carries.
    #[serde(default)]
    pub element: Option<Element>,
    #[serde(default)]
    pub status: Option<String>,
    /// Armor.
    #[serde(default)]
    pub hp: f32,
    #[serde(default)]
    pub defense: f32,
    #[serde(default)]
    pub rarity: u32,
    #[serde(default)]
    pub icon: Option<String>,
}

impl Item {
    /// The icon file name (`assets/ui/icons/<icon>.png`).
    pub fn icon(&self) -> &str {
        self.icon.as_deref().unwrap_or(&self.id)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Recipe {
    pub id: String,
    pub product: String,
    #[serde(default = "one")]
    pub count: u32,
    pub materials: Vec<(String, u32)>,
    pub work: f32,
    #[serde(default = "one_f")]
    pub exp: f32,
    /// The building that crafts it.
    pub station: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Technology {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub desc: String,
    pub level: u32,
    pub cost: u32,
    #[serde(default)]
    pub require: Option<String>,
    #[serde(default)]
    pub builds: Vec<String>,
    #[serde(default)]
    pub recipes: Vec<String>,
}

/// What a finished building does.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub enum Role {
    Station,
    Storage { slots: usize, food_only: bool },
    Core { radius: f32 },
    Ranch,
    Repair,
    JobBoard,
    Bed,
    CreatureBed,
}

/// Who can work a building: a job at some talent rank, how many at once (0 = no limit), whether the
/// player can work it too, and the task it counts as for work order (rules.ron `task_order`).
#[derive(Debug, Clone, PartialEq)]
pub struct JobSlot {
    pub job: String,
    pub rank: u32,
    pub workers: usize,
    pub player: bool,
    pub task: String,
}

/// A field: plant `seed_work`, water `water_work`, wait `grow_s`, harvest `harvest_work` for `count` of `item`.
#[derive(Debug, Clone, PartialEq)]
pub struct Crop {
    pub item: String,
    pub count: u32,
    pub seed_work: f32,
    pub water_work: f32,
    pub harvest_work: f32,
    pub grow_s: f32,
    pub seed: String,
}

#[derive(Debug, Clone, Deserialize)]
struct BuildRaw {
    id: String,
    name: String,
    category: String,
    #[serde(default)]
    desc: String,
    materials: Vec<(String, u32)>,
    work: f32,
    #[serde(default = "one_f")]
    exp: f32,
    #[serde(default)]
    role: Option<Role>,
    #[serde(default)]
    jobs: Vec<(String, u32, usize, bool, String)>,
    #[serde(default)]
    product: Option<(String, f32)>,
    #[serde(default)]
    crop: Option<(String, u32, f32, f32, f32, f32, String)>,
    #[serde(default)]
    max: u32,
    #[serde(default)]
    near_core: bool,
    #[serde(default = "build_hp")]
    hp: f32,
    #[serde(default = "one_f")]
    defense: f32,
}

fn build_hp() -> f32 {
    1000.0
}

#[derive(Debug, Clone)]
pub struct BuildObject {
    pub id: String,
    pub name: String,
    pub category: String,
    pub desc: String,
    pub materials: Vec<(String, u32)>,
    pub work: f32,
    pub exp: f32,
    pub role: Option<Role>,
    pub jobs: Vec<JobSlot>,
    /// (item, work points per item).
    pub product: Option<(String, f32)>,
    pub crop: Option<Crop>,
    /// How many one camp may have (0 = no limit).
    pub max: u32,
    /// Only inside a camp.
    pub near_core: bool,
    pub hp: f32,
    pub defense: f32,
}

impl From<BuildRaw> for BuildObject {
    fn from(r: BuildRaw) -> Self {
        Self {
            id: r.id,
            name: r.name,
            category: r.category,
            desc: r.desc,
            materials: r.materials,
            work: r.work,
            exp: r.exp,
            role: r.role,
            jobs: r.jobs.into_iter().map(|(job, rank, workers, player, task)| JobSlot { job, rank, workers, player, task }).collect(),
            product: r.product,
            crop: r.crop.map(|(item, count, seed_work, water_work, harvest_work, grow_s, seed)| Crop { item, count, seed_work, water_work, harvest_work, grow_s, seed }),
            max: r.max,
            near_core: r.near_core,
            hp: r.hp,
            defense: r.defense,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub enum SkillAction {
    /// A projectile toward the target.
    Shot { speed: f32, radius: f32 },
    /// Curl up and tumble into the target, turning toward it at `homing` rad/s.
    Roll { time: f32, speed: f32, homing: f32, radius: f32 },
}

#[derive(Debug, Clone, Deserialize)]
pub struct Skill {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub desc: String,
    pub element: Element,
    pub power: f32,
    pub min_range: f32,
    pub max_range: f32,
    #[serde(default = "far")]
    pub max_height: f32,
    pub cooldown: f32,
    pub action: SkillAction,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub enum BondKind {
    Shield { duration: f32, cooldown: f32 },
    Carry { weight: f32 },
    None,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Bond {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub desc: String,
    pub kind: BondKind,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Job {
    pub id: String,
    pub name: String,
    /// Work points per second by talent rank (index 0 = can't).
    pub speeds: Vec<f32>,
    #[serde(default)]
    pub hands: bool,
}

/// One primitive of a creature body, in units of the body radius, facing -Z.
#[derive(Debug, Clone, Deserialize)]
pub struct Part {
    /// ball, cube, cyl, cone.
    pub shape: String,
    pub size: (f32, f32, f32),
    pub at: (f32, f32, f32),
    /// Degrees around X, Y, Z.
    #[serde(default)]
    pub rot: (f32, f32, f32),
    /// RRGGBB.
    pub color: String,
    #[serde(default)]
    pub glow: f32,
    /// Also add a copy mirrored across x = 0.
    #[serde(default)]
    pub mirror: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Model {
    /// The main body (flashes when hit).
    pub body: Part,
    #[serde(default)]
    pub parts: Vec<Part>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Creature {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub desc: String,
    pub elements: Vec<Element>,
    pub hp: f32,
    pub melee: f32,
    pub shot: f32,
    pub defense: f32,
    /// Body radius (m).
    pub size: f32,
    pub walk_speed: f32,
    pub run_speed: f32,
    pub sight: f32,
    /// Degrees.
    pub sight_angle: f32,
    pub hearing: f32,
    pub stomach: f32,
    pub stomach_rate: f32,
    pub craft_speed: f32,
    pub haul_speed: f32,
    pub catch_rate: f32,
    pub exp_rate: f32,
    pub male_chance: f32,
    #[serde(default)]
    pub nocturnal: bool,
    #[serde(default)]
    pub hands: bool,
    #[serde(default)]
    pub jobs: BTreeMap<String, u32>,
    pub learnset: Vec<(u32, String)>,
    #[serde(default)]
    pub bond: Option<String>,
    #[serde(default)]
    pub ranch: Option<String>,
    /// (item, chance 0..1, min, max).
    #[serde(default)]
    pub drops: Vec<(String, f32, u32, u32)>,
    pub model: Model,
}

impl Creature {
    /// Talent rank for a job (0 = can't do it).
    pub fn job_rank(&self, job: &str) -> u32 {
        self.jobs.get(job).copied().unwrap_or(0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
pub enum NodeKind {
    Tree,
    Rock,
    Crystal,
    Ore,
    BerryBush,
    Pebble,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NodeDef {
    pub kind: NodeKind,
    pub hp: f32,
    pub defense: f32,
    /// (item, min, max, chance).
    pub batch: Vec<(String, u32, u32, f32)>,
    pub last: Vec<(String, u32, u32, f32)>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub enum Goal {
    Have(String, u32),
    Build(String),
    Unlock(String),
    Craft(String),
    Capture(u32),
    Workers(u32),
    Eat,
    Equip(String),
    SpendPoint,
    Level(u32),
}

#[derive(Debug, Clone, Deserialize)]
pub struct Mission {
    pub title: String,
    pub text: String,
    pub goal: Goal,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct CampNeeds {
    #[serde(default)]
    pub workers: u32,
    /// (any of these buildings, count).
    #[serde(default)]
    pub builds: Vec<(Vec<String>, u32)>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CampLevel {
    pub level: u32,
    pub workers: u32,
    #[serde(default)]
    pub needs: Option<CampNeeds>,
}

/// A wild herd: (creature, centre (x, z), radius, count, (min level, max level)).
pub type Spawn = (String, (f32, f32), f32, u32, (u32, u32));

#[derive(Debug, Clone, Deserialize)]
pub struct World {
    pub nodes: Vec<NodeDef>,
    pub spawns: Vec<Spawn>,
    /// (item, weight, min, max).
    #[serde(default)]
    pub loot: BTreeMap<String, Vec<(String, f32, u32, u32)>>,
    pub missions: Vec<Mission>,
    pub camp_levels: Vec<CampLevel>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StatusDef {
    pub name: String,
    pub duration: f32,
    pub tick_s: f32,
    pub hp_percent: f32,
    pub capture_bonus: f32,
    #[serde(default)]
    pub immune: Vec<Element>,
    /// RRGGBB.
    pub color: String,
}

/// content/rules.ron (the comments there explain each value).
#[derive(Debug, Clone, Deserialize)]
pub struct Rules {
    pub player_hp: f32,
    pub player_stamina: f32,
    pub player_attack: f32,
    pub player_defense: f32,
    pub player_craft_speed: f32,
    pub hunger_max: f32,
    pub hunger_rate: f32,
    pub starving_hp_percent: f32,
    pub carry_weight: f32,
    pub max_level: u32,
    pub tech_points_per_level: u32,
    pub status_points_per_level: u32,
    pub point_hp: f32,
    pub point_stamina: f32,
    pub point_attack: f32,
    pub point_work_speed: f32,
    pub point_weight: f32,
    pub sprint_stamina: f32,
    pub jump_stamina: f32,
    pub glide_stamina: f32,
    pub swim_stamina: f32,
    pub swim_dash_stamina: f32,
    pub start_hour: f32,
    pub time_scale: f32,
    pub night_start: f32,
    pub night_end: f32,
    pub exp_base: f32,
    pub exp_power: f32,
    pub defeat_exp_base: f32,
    pub defeat_exp_per_level: f32,
    pub build_exp: f32,
    pub craft_exp: f32,
    pub capture_bonus_count: u32,
    pub start_unlocked: Vec<String>,
    pub hp_growth_base: f32,
    pub hp_growth_per_level: f32,
    pub hp_flat: f32,
    pub attack_growth_base: f32,
    pub attack_growth_per_level: f32,
    pub attack_flat: f32,
    pub defense_growth_base: f32,
    pub defense_growth_per_level: f32,
    pub defense_flat: f32,
    pub talent_bonus: f32,
    pub damage_scale: f32,
    pub defense_soak: f32,
    pub skill_damage_rate: f32,
    pub creature_to_player_rate: f32,
    pub same_element_rate: f32,
    pub weak_point_rate: f32,
    pub sleeping_rate: f32,
    pub element_strong: f32,
    pub element_weak: f32,
    pub element_chart: Vec<(Element, Element)>,
    pub companion_cooldown_rate: f32,
    pub recall_cooldown: f32,
    pub node_tool_rate: f32,
    pub node_skill_rate: f32,
    pub node_weapon_rate: f32,
    pub node_min_damage: f32,
    pub capture_hp_weight: f64,
    pub capture_level_window: f64,
    pub capture_back_bonus: f64,
    pub shake_weights: Vec<f64>,
    pub statuses: BTreeMap<String, StatusDef>,
    pub creature_hp_regen_percent: f32,
    pub full_threshold: f32,
    pub mood_regen_full: f32,
    pub mood_loss_hungry: f32,
    pub mood_loss_starving: f32,
    pub ground_sleep_mood_rate: f32,
    pub stored_mood_regen: f32,
    pub camp_min_distance: f32,
    pub task_order: Vec<String>,
    pub long_press: f32,
}

#[derive(Debug)]
pub struct GameData {
    pub dir: PathBuf,
    pub rules: Rules,
    pub items: HashMap<String, Item>,
    item_key: HashMap<String, String>,
    pub recipes: Vec<Recipe>,
    /// In unlock-level order.
    pub technologies: Vec<Technology>,
    pub builds: Vec<BuildObject>,
    pub skills: HashMap<String, Skill>,
    pub creatures: HashMap<String, Creature>,
    pub bonds: HashMap<String, Bond>,
    pub jobs: Vec<Job>,
    pub world: World,
}

fn read<T: DeserializeOwned>(dir: &Path, name: &str) -> Result<T> {
    let p = dir.join(format!("{name}.ron"));
    let s = std::fs::read_to_string(&p).with_context(|| format!("reading {}", p.display()))?;
    let opts = ron::Options::default().with_default_extension(ron::extensions::Extensions::IMPLICIT_SOME);
    opts.from_str(&s).with_context(|| format!("parsing {}", p.display()))
}

impl GameData {
    /// The `content` folder, found next to the executable, in the working directory or in the repo.
    pub fn default_dir() -> PathBuf {
        let candidates = [std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.join("content"))), Some(PathBuf::from("content")), Some(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content"))];
        candidates.into_iter().flatten().find(|p| p.join("rules.ron").exists()).unwrap_or_else(|| PathBuf::from("content"))
    }

    pub fn load(dir: &Path) -> Result<Self> {
        let items: Vec<Item> = read(dir, "items")?;
        let builds: Vec<BuildRaw> = read(dir, "buildings")?;
        let mut technologies: Vec<Technology> = read(dir, "tech")?;
        technologies.sort_by(|a, b| a.level.cmp(&b.level));
        let skills: Vec<Skill> = read(dir, "skills")?;
        let creatures: Vec<Creature> = read(dir, "creatures")?;
        let bonds: Vec<Bond> = read(dir, "bonds")?;
        let d = GameData {
            dir: dir.to_owned(),
            rules: read(dir, "rules")?,
            item_key: items.iter().map(|i| (i.id.to_ascii_lowercase(), i.id.clone())).collect(),
            items: items.into_iter().map(|i| (i.id.clone(), i)).collect(),
            recipes: read(dir, "recipes")?,
            technologies,
            builds: builds.into_iter().map(BuildObject::from).collect(),
            skills: skills.into_iter().map(|s| (s.id.clone(), s)).collect(),
            creatures: creatures.into_iter().map(|c| (c.id.clone(), c)).collect(),
            bonds: bonds.into_iter().map(|b| (b.id.clone(), b)).collect(),
            jobs: read(dir, "jobs")?,
            world: read(dir, "world")?,
        };
        let problems = d.validate();
        if !problems.is_empty() {
            anyhow::bail!("content problems in {}:\n  {}", dir.display(), problems.join("\n  "));
        }
        Ok(d)
    }

    /// Canonical item id for any spelling.
    pub fn item_id(&self, id: &str) -> Option<&str> {
        self.item_key.get(&id.to_ascii_lowercase()).map(String::as_str)
    }

    pub fn item(&self, id: &str) -> Option<&Item> {
        self.item_id(id).and_then(|k| self.items.get(k))
    }

    pub fn recipe(&self, id: &str) -> Option<&Recipe> {
        self.recipes.iter().find(|r| r.id.eq_ignore_ascii_case(id))
    }

    pub fn build(&self, id: &str) -> Option<&BuildObject> {
        self.builds.iter().find(|b| b.id.eq_ignore_ascii_case(id))
    }

    pub fn tech(&self, id: &str) -> Option<&Technology> {
        self.technologies.iter().find(|t| t.id.eq_ignore_ascii_case(id))
    }

    pub fn job(&self, id: &str) -> Option<&Job> {
        self.jobs.iter().find(|j| j.id == id)
    }

    pub fn node(&self, kind: NodeKind) -> Option<&NodeDef> {
        self.world.nodes.iter().find(|n| n.kind == kind)
    }

    pub fn camp_level(&self, level: u32) -> Option<&CampLevel> {
        self.world.camp_levels.iter().find(|l| l.level == level)
    }

    /// Whether the game can be played through with this content: every material, mission item and
    /// buildable thing can be obtained in the world (resource nodes, creature drops, ranch loot, crops,
    /// recipes whose station can be built), every building with jobs can be worked by a creature that
    /// spawns in the world, and every technology is reachable by level. Readable lines, empty = all good.
    pub fn progression_problems(&self) -> Vec<String> {
        use std::collections::HashSet;
        let mut out = Vec::new();
        let spawned: HashSet<&str> = self.world.spawns.iter().map(|s| s.0.as_str()).collect();
        let mut have: HashSet<String> = HashSet::new();
        for n in &self.world.nodes {
            have.extend(n.batch.iter().chain(&n.last).map(|r| r.0.clone()));
        }
        for c in self.creatures.values().filter(|c| spawned.contains(c.id.as_str())) {
            have.extend(c.drops.iter().map(|d| d.0.clone()));
            if let Some(rows) = c.ranch.as_ref().and_then(|l| self.world.loot.get(l)) {
                have.extend(rows.iter().map(|r| r.0.clone()));
            }
        }
        let unlockable = |kind: &str, id: &str| self.technologies.iter().any(|t| t.level <= self.rules.max_level && if kind == "b" { t.builds.iter().any(|b| b == id) } else { t.recipes.iter().any(|r| r == id) });
        let affordable = |mats: &[(String, u32)], have: &HashSet<String>| mats.iter().all(|(m, _)| have.contains(m));
        // Grow the set until nothing new can be made: a recipe needs its materials and a buildable station,
        // a crop needs a buildable field and its seed.
        loop {
            let before = have.len();
            let buildable = |id: &str, have: &HashSet<String>| self.build(id).is_some_and(|b| (unlockable("b", &b.id) || self.rules.start_unlocked.iter().any(|t| self.tech(t).is_some_and(|t| t.builds.contains(&b.id)))) && affordable(&b.materials, have));
            let mut new = Vec::new();
            for r in &self.recipes {
                if unlockable("r", &r.id) && buildable(&r.station, &have) && affordable(&r.materials, &have) {
                    new.push(r.product.clone());
                }
            }
            for b in &self.builds {
                if let Some(c) = &b.crop
                    && buildable(&b.id, &have)
                    && have.contains(&c.seed)
                {
                    new.push(c.item.clone());
                }
                if let Some((p, _)) = &b.product
                    && buildable(&b.id, &have)
                {
                    new.push(p.clone());
                }
            }
            have.extend(new);
            if have.len() == before {
                break;
            }
        }
        let missing = |id: &str| !have.contains(id);
        for r in &self.recipes {
            for (m, _) in r.materials.iter().filter(|(m, _)| missing(m)) {
                out.push(format!("recipe {}: material \"{m}\" can't be obtained", r.id));
            }
            if !unlockable("r", &r.id) {
                out.push(format!("recipe {}: no technology unlocks it", r.id));
            }
        }
        for b in &self.builds {
            for (m, _) in b.materials.iter().filter(|(m, _)| missing(m)) {
                out.push(format!("building {}: material \"{m}\" can't be obtained", b.id));
            }
            if !unlockable("b", &b.id) {
                out.push(format!("building {}: no technology unlocks it", b.id));
            }
            for j in &b.jobs {
                let needs_hands = self.job(&j.job).is_some_and(|x| x.hands);
                let worker = self.creatures.values().any(|c| spawned.contains(c.id.as_str()) && c.job_rank(&j.job) >= j.rank && (!needs_hands || c.hands));
                if !worker && !j.player {
                    out.push(format!("building {}: no spawning creature can do its \"{}\" job (rank {})", b.id, j.job, j.rank));
                }
            }
        }
        for m in &self.world.missions {
            if let Goal::Have(i, _) | Goal::Craft(i) | Goal::Equip(i) = &m.goal
                && missing(i)
            {
                out.push(format!("mission \"{}\": \"{i}\" can't be obtained", m.title));
            }
        }
        for t in &self.technologies {
            if t.level > self.rules.max_level {
                out.push(format!("tech {}: level {} is above the max level", t.id, t.level));
            }
        }
        for c in self.creatures.values().filter(|c| !spawned.contains(c.id.as_str())) {
            out.push(format!("creature {}: never spawns (world.ron spawns)", c.id));
        }
        out
    }

    /// Every reference to an id that doesn't exist, as readable lines (empty = all good).
    pub fn validate(&self) -> Vec<String> {
        let mut out = Vec::new();
        let item = |id: &str, what: String, out: &mut Vec<String>| {
            if self.item(id).is_none() {
                out.push(format!("{what}: unknown item \"{id}\""));
            }
        };
        for r in &self.recipes {
            item(&r.product, format!("recipe {}", r.id), &mut out);
            for (m, _) in &r.materials {
                item(m, format!("recipe {}", r.id), &mut out);
            }
            if self.build(&r.station).is_none_or(|b| b.role != Some(Role::Station)) {
                out.push(format!("recipe {}: station \"{}\" is not a building with role Station", r.id, r.station));
            }
        }
        for b in &self.builds {
            for (m, _) in &b.materials {
                item(m, format!("building {}", b.id), &mut out);
            }
            for j in &b.jobs {
                if self.job(&j.job).is_none() {
                    out.push(format!("building {}: unknown job \"{}\"", b.id, j.job));
                }
                if !self.rules.task_order.contains(&j.task) {
                    out.push(format!("building {}: task \"{}\" is not in rules.ron task_order", b.id, j.task));
                }
            }
            if let Some((p, _)) = &b.product {
                item(p, format!("building {} product", b.id), &mut out);
            }
            if let Some(c) = &b.crop {
                item(&c.item, format!("building {} crop", b.id), &mut out);
                item(&c.seed, format!("building {} crop seed", b.id), &mut out);
            }
        }
        for t in &self.technologies {
            for b in &t.builds {
                if self.build(b).is_none() {
                    out.push(format!("tech {}: unknown building \"{b}\"", t.id));
                }
            }
            for r in &t.recipes {
                if self.recipe(r).is_none() {
                    out.push(format!("tech {}: unknown recipe \"{r}\"", t.id));
                }
            }
            if let Some(req) = &t.require
                && self.tech(req).is_none()
            {
                out.push(format!("tech {}: requires unknown tech \"{req}\"", t.id));
            }
        }
        for c in self.creatures.values() {
            for (_, s) in &c.learnset {
                if !self.skills.contains_key(s) {
                    out.push(format!("creature {}: unknown skill \"{s}\"", c.id));
                }
            }
            for j in c.jobs.keys() {
                if self.job(j).is_none() {
                    out.push(format!("creature {}: unknown job \"{j}\"", c.id));
                }
            }
            if let Some(b) = &c.bond
                && !self.bonds.contains_key(b)
            {
                out.push(format!("creature {}: unknown bond skill \"{b}\"", c.id));
            }
            if let Some(l) = &c.ranch
                && !self.world.loot.contains_key(l)
            {
                out.push(format!("creature {}: unknown loot table \"{l}\"", c.id));
            }
            for (d, ..) in &c.drops {
                item(d, format!("creature {} drop", c.id), &mut out);
            }
        }
        for i in self.items.values() {
            if let Some(s) = &i.status
                && !self.rules.statuses.contains_key(s)
            {
                out.push(format!("item {}: unknown status \"{s}\"", i.id));
            }
        }
        for (name, rows) in &self.world.loot {
            for (it, ..) in rows {
                item(it, format!("loot {name}"), &mut out);
            }
        }
        for n in &self.world.nodes {
            for (it, ..) in n.batch.iter().chain(&n.last) {
                item(it, format!("node {:?}", n.kind), &mut out);
            }
        }
        for (c, ..) in &self.world.spawns {
            if !self.creatures.contains_key(c) {
                out.push(format!("spawn: unknown creature \"{c}\""));
            }
        }
        for t in &self.rules.start_unlocked {
            if self.tech(t).is_none() {
                out.push(format!("rules start_unlocked: unknown tech \"{t}\""));
            }
        }
        for m in &self.world.missions {
            let bad = match &m.goal {
                Goal::Have(i, _) | Goal::Craft(i) | Goal::Equip(i) => self.item(i).is_none(),
                Goal::Build(b) => self.build(b).is_none(),
                Goal::Unlock(t) => self.tech(t).is_none(),
                _ => false,
            };
            if bad {
                out.push(format!("mission \"{}\": goal {:?} refers to something unknown", m.title, m.goal));
            }
        }
        for l in &self.world.camp_levels {
            for (ids, _) in l.needs.iter().flat_map(|n| &n.builds) {
                for b in ids {
                    if self.build(b).is_none() {
                        out.push(format!("camp level {}: unknown building \"{b}\"", l.level));
                    }
                }
            }
        }
        out
    }
}

impl crate::inventory::ItemInfo for GameData {
    fn max_stack(&self, item: &str) -> u32 {
        self.item(item).map_or(999, |i| i.max_stack)
    }
    fn weight(&self, item: &str) -> f32 {
        self.item(item).map_or(0.0, |i| i.weight)
    }
    fn durability(&self, item: &str) -> Option<f32> {
        self.item(item).and_then(|i| i.durability)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn data() -> GameData {
        GameData::load(&GameData::default_dir()).expect("content loads")
    }

    #[test]
    fn content_can_be_played_through() {
        let d = data();
        let p = d.progression_problems();
        assert!(p.is_empty(), "{}", p.join("
"));
    }

    #[test]
    fn progression_check_catches_gaps() {
        // Without wild creatures nothing drops hides or ember sacs and no one works the camp.
        let mut d = data();
        d.world.spawns.clear();
        let p = d.progression_problems();
        assert!(p.iter().any(|l| l.contains("ember_sac")) && p.iter().any(|l| l.contains("no spawning creature")), "{p:?}");
        // Without the copper ore node, ingots, gear parts and the better orbs can't be made.
        let mut d = data();
        d.world.nodes.retain(|n| n.kind != NodeKind::Ore);
        let p = d.progression_problems();
        assert!(p.iter().any(|l| l.contains("copper_ingot")), "{p:?}");
    }

    #[test]
    fn content_loads_and_is_consistent() {
        let d = data();
        assert!(d.validate().is_empty(), "{:?}", d.validate());
        assert!(d.items.len() > 30 && d.recipes.len() > 20 && d.technologies.len() > 20 && d.builds.len() > 40);
        assert_eq!(d.creatures.len(), 5);
        assert_eq!(d.item_id("Capture_Orb"), Some("capture_orb"));
        assert_eq!(d.build("workbench").unwrap().role, Some(Role::Station));
        assert!(d.build("berry_patch").unwrap().crop.is_some());
    }

    #[test]
    fn every_creature_has_a_first_skill() {
        let d = data();
        for c in d.creatures.values() {
            assert!(c.learnset.first().is_some_and(|(lv, _)| *lv == 1), "{}", c.id);
        }
    }
}
