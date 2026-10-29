# Capture Kit (Bevy)

An open-source starter kit for **creature-capture survival games**, written in Rust on [Bevy](https://bevyengine.org)
0.19 with [avian](https://github.com/Jondolf/avian) physics. Explore an island, gather and craft, weaken wild
creatures and catch them with orbs, then found a camp where your creatures live and work for you.

Everything here is original: the code, the five creatures, the items, buildings, numbers, icons and sounds. Fork it,
rename it, replace the content with your own and ship your own game. See [LEGAL.md](LEGAL.md).

## What's in it

- **Third-person player:** walk, sprint, crouch, jump, roll, climb, glide, swim, stamina and hunger, fall damage,
  slope handling.
- **Combat:** melee weapons and tools, a bow with three arrow types (plain, fire, venom), burning and poison status
  effects, damage numbers, element strengths and weaknesses.
- **Capture:** throw orbs at weakened creatures; the chance depends on their HP, level, species, status effects and
  whether they noticed you. One to three shakes, then caught or broken free.
- **Creatures:** five original species built from primitive shapes defined in data, with levels, talents, skills
  they learn and equip, a bond skill, wild AI (wander, notice, curious, fight, sleep at night) and a party of five.
- **Camp:** a Camp Core that sets the camp's area, creatures that pick work by their own job priorities (crafting,
  smelting, cooking, farming, mining, chopping, hauling, ranching), feeding, mood, beds, a Job Board, camp levels.
- **Building and crafting:** about 50 buildings with placement rules, blueprints finished by work, dismantling;
  workbench, campfire and smelter recipes, a technology tree unlocked with points from levelling.
- **UI:** HUD, inventory with equipment slots, technology, build menu, crafting stations, chests, Camp Core storage
  and creature details, missions, save/load (F5 / F9), settings (Esc).

## Make it yours

All game content lives in readable [RON](https://github.com/ron-rs/ron) files in [`content/`](content/): creatures
(stats, jobs, skills, and the body model as a list of primitives), items, recipes, buildings, technology, skills,
bond skills, jobs, the world (resource nodes, wild herds, missions, camp levels) and every tuning number in
`rules.ron`. The game checks every reference on start and tells you exactly what is wrong. See
[docs/CONTENT.md](docs/CONTENT.md).

## Build and run

```sh
cargo run --release
```

Needs Rust 1.95 or newer. Details, options and the scripted checks: [docs/BUILDING.md](docs/BUILDING.md).
Controls: [docs/HOW-TO-PLAY.md](docs/HOW-TO-PLAY.md).

## Layout

| Path | What |
| --- | --- |
| `crates/ck_core` | Rules without Bevy: content loading and validation, stats, damage, capture, experience, inventory. Unit tested. |
| `crates/ck_game` | The Bevy game (binary `capture-kit`). |
| `content/` | The game content (RON). |
| `assets/` | Icons (`ui/icons`) and sounds (`sfx`). |
| `tools/` | Sound processing (`sfx/`), the third-party licence generator. |

## Licence

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option. This covers the code, the
content files, and, to the extent the maintainer holds rights in them, the icons and sounds (see
[CREDITS.md](CREDITS.md)). Dependencies keep their own licences: [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md).
