# Content guide

All game content is data in `content/*.ron` ([RON](https://github.com/ron-rs/ron): like JSON with Rust syntax,
comments allowed, `Some(x)` may be written as `x`). Each file starts with a comment block that explains its fields;
this guide is the overview. Ids are lower-case words with underscores and are matched case-insensitively.

On start the game loads every file and checks every cross-reference (items in recipes, buildings in technology,
skills in learnsets, jobs in buildings, ...). If anything is wrong it prints the full list and exits:

```
content problems in content:
  recipe fire_arrow: unknown item "ember_sack"
  tech archery: unknown recipe "long_bow"
```

`cargo test -p ck_core` runs the same check.

## Files

| File | What |
| --- | --- |
| `items.ron` | Every item: materials, food, weapons and tools, armour, gliders, orbs, ammo. |
| `recipes.ron` | What each station crafts: product, count, materials, work points, the station building. |
| `buildings.ron` | Every building: build-menu category, materials, work, what it does (`role`), who can work it (`jobs`), production (`product`) or farming (`crop`). |
| `tech.ron` | The technology tree: unlock level, point cost, prerequisite, unlocked buildings and recipes. |
| `creatures.ron` | Species: stats, elements, size and speeds, senses, needs, jobs, skills by level, bond skill, drops, ranch loot, and the body model. |
| `skills.ron` | Creature skills: element, power, ranges, cooldown, and the action (`Shot` or `Roll`). |
| `bonds.ron` | Bond skills (`Shield`, `Carry`, `None`). |
| `jobs.ron` | Camp jobs and their work speed per talent rank. |
| `world.ron` | The sample world: resource nodes, wild herds, loot tables, the mission chain, camp levels. |
| `rules.ron` | Every tuning number: player stats, stamina, time of day, experience curve, stat growth, damage, elements, capture, statuses, camp needs, work order. |

## Recipes

### Add an item and a recipe

```ron
// items.ron
(id: "bone_knife", name: "Bone Knife", kind: "Weapon", sub: "Melee", attack: 35, durability: 120, weight: 2, icon: "stone_spear"),
// recipes.ron
(id: "bone_knife", product: "bone_knife", materials: [("hide", 1), ("stone", 2)], work: 150, station: "workbench"),
// tech.ron: unlock it
(id: "bone_tools", name: "Bone Tools", desc: "Sharp and light.", level: 4, cost: 2, recipes: ["bone_knife"]),
```

Add `assets/ui/icons/bone_knife.png` (128 x 128, transparent) and drop the `icon` line, or point `icon` at an
existing picture.

### Add a creature

Copy one entry in `creatures.ron` and change it. The body is built from primitives, in units of the creature's body
radius (`size`, metres), facing -Z (forward), y up:

```ron
model: (
    body: (shape: "ball", size: (1.0, 0.9, 1.0), at: (0.0, 0.95, 0.0), color: "F3EBDD"),
    parts: [
        (shape: "ball", size: (0.08, 0.1, 0.05), at: (0.25, 1.2, -0.9), color: "1E1A16", mirror: true), // eyes
        (shape: "cone", size: (0.2, 0.5, 0.2), at: (0.0, 1.8, -0.2), rot: (-20.0, 0.0, 0.0), color: "FFB02E", glow: 4.0),
    ],
),
```

- `shape`: `ball` (size = radii), `cube` (size = half extents), `cyl` and `cone` (size = radius, half height,
  radius).
- `rot`: degrees around X, Y, Z. `mirror: true` adds a copy at -x. `glow` makes it emissive. The `body` part
  flashes when the creature is hit.
- Then add it to a herd in `world.ron` `spawns`, and give it a portrait: `assets/ui/icons/creature_<id>.png`.

Jobs are talent ranks (`{"mine": 2}`); the work speed is `jobs.ron` speed at that rank x `craft_speed` / 100. Jobs
marked `hands: true` (craft, brew) need `hands: true` on the creature. `learnset` lists `(level, skill)`; the first
entry should be level 1. `catch_rate` scales the capture chance (0 = can't be captured).

### Add a building

Add the entry in `buildings.ron` and its look in `crates/ck_game/src/build_models.rs` (`model()`, matched by id;
unknown ids get a crate). The look is a list of boxes, cylinders, balls and cones with colours; the footprint
(collider, placement check) is their bounding box. Icon: `assets/ui/icons/build_<id>.png`.

`role` makes it work: `Station` (crafts the recipes whose `station` is this id), `Storage(slots, food_only)`,
`Core(radius)` (the camp centre; one per camp), `Ranch`, `Repair`, `JobBoard`, `Bed`, `CreatureBed`. A building
with `product` is a producer (creatures with its job make one item per `product` work); one with `crop` is a field.
`jobs` says which job at which talent rank can work it, how many at once, whether the player can help, and which
`task` it counts as for the work order in `rules.ron`.

### Missions and camp levels

`world.ron` `missions` run in order. Goals: `Have(item, count)`, `Build(building)`, `Unlock(tech)`, `Craft(item)`,
`Capture(count)`, `Workers(count)`, `Eat`, `Equip(item)`, `SpendPoint`, `Level(level)`. `camp_levels` sets how
many creatures may live at the camp and what it takes to reach the next level.

## Formulas

All in `crates/ck_core` with the numbers from `rules.ron`:

- **Creature stats** (`stats.rs`): stat = species stat x (growth_base + growth_per_level x level) x (1 + talent_bonus
  x talent / 100) + flat.
- **Damage** (`stats.rs`): damage_scale x power x attack / (defense + defense_soak), times the element multiplier
  (`element_strong` / `element_weak` per defending element in `element_chart`), the same-element bonus and the
  other rates; at least 1.
- **Capture** (`capture.rs`): chance = (1 - capture_hp_weight x hp / max hp) x level factor x catch_rate x (1 +
  status bonus) [x capture_back_bonus if unnoticed], where the level factor is
  clamp((orb level - creature level + window) / (2 x window), 0, 1). Each of the shakes passes with
  chance ^ weight (`shake_weights`, summing to 1), so all pass with exactly the chance.
- **Experience** (`creature.rs`): the next level needs exp_base x level ^ exp_power.

Feel and timing constants that are not content (swing timing, arrow speed, pickup radius, ...) are in
`crates/ck_core/src/consts.rs`; player movement and camera in `crates/ck_game/src/tuning.rs`.
