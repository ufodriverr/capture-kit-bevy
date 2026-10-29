# Building

## Requirements

- Rust 1.95 or newer (`rustup update`).
- Windows, Linux or macOS with a GPU that supports Vulkan, DirectX 12 or Metal.
- Linux: the usual Bevy system packages (ALSA and udev development files, e.g. `libasound2-dev libudev-dev` on
  Debian/Ubuntu); see Bevy's setup guide.

## Run

```sh
cargo run --release
```

The game finds `content/` and `assets/` next to the executable, in the working directory, or in the repository.
To ship a build, copy the executable together with the `content/` and `assets/` folders, `LICENSE-MIT`,
`LICENSE-APACHE`, `THIRD-PARTY-NOTICES.md` and the `licenses/` folder.

## Options

| Option | What |
| --- | --- |
| `--sandbox` | Sandbox: every material cost capped at 1, start at the max level, free crafting on |
| `--visible` | Show the window for `--auto` and `--screenshot` runs (hidden by default) |
| `--screenshot <file.png> [--frames n]` | Render n frames (default 90), save a screenshot, exit |
| `--auto <script>` | Run a scripted check (below) |
| `--seed <n>` | Random seed for scripted runs |

## Checks

```sh
cargo test --workspace            # rules, content validation, formulas
cargo run --release -- --auto capture
```

`--auto <name>` plays a scripted scene with a fixed time step, logs what happens and saves screenshots. Scripts are
in `crates/ck_game/src/scripts.rs`: move, climb, glide, swim, fall, fight, capture, companion, gather, base, models,
dismantle, aim, detour, basetools, settings, slope, gear, status, pickup, steps, settle, inspect, save, night,
menus, work, pose.

Content mistakes (a recipe using an unknown item, a mission pointing at a missing building, ...) stop the game at
start with a list of every problem; `cargo test -p ck_core` checks the same.

## Third-party licences

After changing dependencies, regenerate the notices: `python -I tools/licenses.py`.

## Sounds

`python -I tools/sfx/process.py` rebuilds `assets/sfx/*.ogg` from the raw takes in `tools/sfx/raw/` (needs
ffmpeg). Prompts and settings: `tools/sfx/PROMPTS.md`.
