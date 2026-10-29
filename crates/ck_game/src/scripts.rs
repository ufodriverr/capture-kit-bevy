//! Built-in `--auto` scripts. Each step's end state goes to stdout and `target/shots/<script>.log`.

use bevy::prelude::*;

use crate::devtools::Step;
use crate::input::PlayerInput;
use crate::kinematic::{Mover, yaw_of};
use crate::player::{Health, Locomotion, Mode, Player, Stamina};
use crate::world::{LAKE, PLATEAU, Terrain};

pub const NAMES: &[&str] = &["move", "climb", "glide", "swim", "fall", "fight", "capture", "companion", "gather", "base", "menus", "night", "save", "inspect", "pose", "work", "models", "dismantle", "aim", "detour", "basetools", "settle", "settings", "slope", "gear", "steps", "status", "pickup"];

fn hold(secs: f32, label: &'static str, f: impl FnOnce(&mut PlayerInput)) -> Step {
    let mut input = PlayerInput::default();
    f(&mut input);
    Step { secs, input, label, ..default() }
}

fn fwd(i: &mut PlayerInput) {
    i.move_axis = Vec2::Y;
}

pub fn script(name: &str) -> Option<Vec<Step>> {
    let toward_plateau = (PLATEAU - Vec2::ZERO).normalize();
    let cliff_base = PLATEAU - toward_plateau * 47.0;
    let face_cliff = yaw_of(Vec3::new(toward_plateau.x, 0.0, toward_plateau.y));
    Some(match name {
        "move" => vec![
            Step { teleport: Some((Vec2::new(5.0, 5.0), None)), yaw: Some(0.0), pitch: Some(-0.2), ..hold(1.0, "idle", |_| {}) },
            hold(2.0, "walk 2s", fwd),
            hold(2.0, "sprint 2s", |i| {
                fwd(i);
                i.sprint = true;
            }),
            Step { shot: Some("sprint"), ..hold(0.05, "sprint shot", |i| {
                fwd(i);
                i.sprint = true;
            }) },
            hold(1.0, "release (braking)", |_| {}),
            hold(1.2, "jump", |i| i.jump_pressed = true),
            hold(0.1, "crouch toggle", |i| i.crouch_pressed = true),
            Step { shot: Some("crouch"), ..hold(1.5, "crouch walk", fwd) },
            hold(0.1, "uncrouch", |i| i.crouch_pressed = true),
            hold(0.8, "roll", |i| {
                fwd(i);
                i.roll_pressed = true;
            }),
            hold(1.5, "sprint again", |i| {
                fwd(i);
                i.sprint = true;
            }),
            Step { shot: Some("slide"), ..hold(0.4, "slide", |i| {
                fwd(i);
                i.sprint = true;
                i.crouch_pressed = true;
            }) },
            hold(1.5, "slide out", |_| {}),
            Step { shot: Some("aim"), ..hold(1.0, "aim walk", |i| {
                fwd(i);
                i.aim = true;
            }) },
            hold(14.0, "sprint until exhausted", |i| {
                fwd(i);
                i.sprint = true;
            }),
            hold(0.1, "jump while exhausted", |i| i.jump_pressed = true),
        ],
        "climb" => vec![
            Step { teleport: Some((cliff_base, None)), yaw: Some(face_cliff), pitch: Some(0.1), ..hold(0.5, "at cliff base", |_| {}) },
            hold(3.0, "walk to cliff", fwd),
            hold(0.6, "jump at wall", |i| {
                fwd(i);
                i.jump_pressed = true;
            }),
            Step { shot: Some("climbing"), ..hold(2.0, "climb up", fwd) },
            hold(0.1, "let go of the keys", |_| {}),
            hold(2.0, "hang still (no drift)", |_| {}),
            hold(1.0, "climb sideways", |i| i.move_axis = Vec2::X),
            hold(16.0, "keep climbing / top out", fwd),
            Step { shot: Some("top"), ..hold(1.0, "on top", |_| {}) },
        ],
        "glide" => vec![
            Step { teleport: Some((Vec2::new(0.0, 20.0), Some(40.0))), yaw: Some(0.0), setup: Some(give_glider), ..hold(0.4, "drop from 36 m", |_| {}) },
            Step { shot: Some("gliding"), ..hold(3.0, "glide", |i| {
                fwd(i);
                i.jump_pressed = true;
                i.jump_held = true;
            }) },
            hold(30.0, "glide to ground", |i| {
                fwd(i);
                i.jump_held = true;
            }),
        ],
        "swim" => vec![
            Step { teleport: Some((LAKE, Some(0.5))), yaw: Some(0.0), ..hold(2.0, "into lake", |_| {}) },
            Step { shot: Some("swimming"), ..hold(3.0, "swim", fwd) },
            hold(3.0, "dash swim", |i| {
                fwd(i);
                i.sprint = true;
            }),
        ],
        "fall" => vec![
            Step { teleport: Some((Vec2::new(10.0, 10.0), Some(34.0))), ..hold(4.0, "drop ~30 m", |_| {}) },
            Step { teleport: Some((Vec2::new(12.0, 10.0), Some(12.0))), setup: Some(heal), ..hold(3.0, "drop ~8 m", |_| {}) },
        ],
        "fight" => {
            let mut v = vec![Step { setup: Some(approach_creature), pitch: Some(-0.1), ..hold(0.3, "next to a Fluffin", |_| {}) }];
            for i in 0..12 {
                v.push(Step { setup: Some(face_creature), shot: (i == 2).then_some("punch"), ..hold(0.6, "punch", |i| i.attack_pressed = true) });
            }
            v.push(Step { shot: Some("after"), ..hold(3.0, "watch", |_| {}) });
            v
        }
        "capture" => vec![
            Step { setup: Some(capture_setup), pitch: Some(-0.12), ..hold(1.0, "6 m from a Fluffin, 5 orbs", |_| {}) },
            Step { setup: Some(face_creature), shot: Some("aim"), ..hold(0.6, "aim orb", |i| i.throw_held = true) },
            hold(4.5, "judging", |_| {}),
            Step { setup: Some(face_creature), ..hold(0.6, "aim again", |i| i.throw_held = true) },
            hold(4.5, "judging", |_| {}),
            Step { setup: Some(face_creature), ..hold(0.6, "aim again", |i| i.throw_held = true) },
            hold(4.5, "judging", |_| {}),
        ],
        "companion" => vec![
            Step { setup: Some(companion_setup), teleport: Some((Vec2::new(5.0, 5.0), None)), yaw: Some(0.0), ..hold(0.5, "own a Lv8 Fluffin", |_| {}) },
            Step { shot: Some("summoned"), ..hold(1.5, "summon (E)", |i| i.summon_pressed = true) },
            hold(4.0, "walk, it follows", fwd),
            hold(3.0, "sprint, it catches up", |i| {
                fwd(i);
                i.sprint = true;
            }),
            Step { setup: Some(approach_creature_far), ..hold(0.5, "near wild Fluffins", |_| {}) },
            Step { setup: Some(face_creature), ..hold(0.3, "direct attack order", |i| i.direct_attack_pressed = true) },
            Step { shot: Some("fighting"), ..hold(6.0, "companion fights", |_| {}) },
            hold(6.0, "companion fights more", |_| {}),
            hold(0.2, "bond skill (F)", |i| i.partner_pressed = true),
            Step { shot: Some("shield"), ..hold(2.0, "shield up", |_| {}) },
            hold(0.2, "bond skill off", |i| i.partner_pressed = true),
            hold(1.5, "recall (E)", |i| i.summon_pressed = true),
        ],
        "gather" => {
            let mut v = vec![Step { setup: Some(gather_setup), ..hold(0.3, "at a tree with a stone axe", |_| {}) }];
            for _ in 0..5 {
                v.push(Step { setup: Some(face_node_tree), ..hold(0.6, "chop", |i| i.attack_pressed = true) });
            }
            v.push(Step { setup: Some(go_rock), ..hold(0.3, "at a rock with a pickaxe", |i| i.weapon_scroll = 1) });
            for _ in 0..16 {
                v.push(Step { setup: Some(face_node_rock), ..hold(0.6, "mine", |i| i.attack_pressed = true) });
            }
            v.push(Step { setup: Some(go_bush), ..hold(0.3, "at a berry bush", |_| {}) });
            v.push(hold(0.3, "pick (F)", |i| i.interact_pressed = true));
            v.push(Step { shot: Some("done"), ..hold(0.5, "done", |_| {}) });
            v
        }
        "base" => {
            let place = |id: &'static str, yaw: f32| -> Vec<Step> {
                vec![
                    Step { setup: Some(match id {
                        "camp_core" => (|w: &mut World| start_placing(w, "camp_core")) as fn(&mut World),
                        "workbench" => |w: &mut World| start_placing(w, "workbench"),
                        "feed_box" => |w: &mut World| start_placing(w, "feed_box"),
                        "storage_chest" => |w: &mut World| start_placing(w, "storage_chest"),
                        _ => |w: &mut World| start_placing(w, "ranch"),
                    }), yaw: Some(yaw), pitch: Some(-0.55), ..hold(0.4, "aim the ghost", |_| {}) },
                    hold(0.1, "place (LMB)", |i| i.attack_pressed = true),
                    hold(0.5, "step closer", fwd),
                ]
            };
            let mut v = vec![Step { setup: Some(base_setup), teleport: Some((Vec2::new(8.0, 8.0), None)), ..hold(0.5, "materials, Lv5, techs, a Lv5 Tinkit and a Lv5 Fluffin", |_| {}) }];
            v.extend(place("camp_core", 0.0));
            v.push(Step { shot: Some("camp_core_building"), ..hold(11.0, "build the Camp Core (hold F)", |i| i.interact_held = true) });
            v.extend(place("workbench", 1.57));
            v.push(hold(6.0, "build the workbench (hold F)", |i| i.interact_held = true));
            v.extend(place("storage_chest", std::f32::consts::PI));
            v.push(hold(11.0, "build the chest (hold F)", |i| i.interact_held = true));
            v.push(Step { setup: Some(assign_worker), ..hold(1.0, "Tinkit and Fluffin -> camp workers", |_| {}) });
            v.push(Step { setup: Some(queue_orbs), ..hold(1.0, "queue 3 Capture Orbs", |_| {}) });
            v.push(Step { shot: Some("worker"), ..hold(10.0, "the Tinkit crafts", |_| {}) });
            v.push(hold(10.0, "the Tinkit crafts more", |_| {}));
            v.push(Step { setup: Some(collect_bench), ..hold(1.0, "collect", |_| {}) });
            v.push(Step { teleport: Some((Vec2::new(14.0, -2.0), None)), ..hold(0.3, "walk away from the buildings", |_| {}) });
            v.extend(place("ranch", 0.8));
            v.push(Step { setup: Some(finish_all), ..hold(2.0, "(ranch finished by the script)", |_| {}) });
            v.push(Step { shot: Some("ranch"), ..hold(40.0, "Fluffin at the ranch", |_| {}) });
            // Stand back so auto pickup leaves the ranch goods for the haulers.
            v.push(Step { teleport: Some((Vec2::new(-10.0, -14.0), None)), ..hold(0.2, "step back from the ranch", |_| {}) });
            v.push(hold(70.0, "fluff appears", |_| {}));
            v.push(Step { setup: Some(dismantle_ranch), shot: Some("transport"), ..hold(30.0, "ranch dismantled: a free worker hauls the fluff to the chest", |_| {}) });
            v
        }
        "models" => {
            // Every build object, finished, in rows of MODEL_COLS; one shot per 4 rows from behind.
            let rows = MODEL_ROWS;
            let mut v = vec![Step { setup: Some(models_setup), ..hold(0.5, "all build objects in a grid", |_| {}) }];
            for (k, shot) in ["models_1", "models_2", "models_3", "models_4", "models_5"].into_iter().enumerate().take(rows.div_ceil(4)) {
                let z = MODELS_AT.y - (k * 4) as f32 * MODEL_GAP + 9.0;
                v.push(Step { teleport: Some((Vec2::new(MODELS_AT.x, z), None)), yaw: Some(0.0), pitch: Some(-0.3), shot: Some(shot), ..hold(1.0, "rows", |_| {}) });
            }
            v
        }
        "dismantle" => vec![
            Step { setup: Some(dismantle_setup), yaw: Some(0.0), pitch: Some(-0.32), ..hold(0.6, "a workbench and a chest with 7 Wood inside, in front", |_| {}) },
            Step { shot: Some("mode"), ..hold(0.4, "X: dismantle mode", |i| i.dismantle_pressed = true) },
            hold(0.6, "aim", |_| {}),
            hold(1.4, "hold LMB", |i| i.attack_held = true),
            Step { setup: Some(|w| report_bag(w, "after the first")), pitch: Some(-0.1), ..hold(0.4, "next one (further: look up a bit)", |_| {}) },
            hold(1.4, "hold LMB again", |i| i.attack_held = true),
            Step { setup: Some(|w| report_bag(w, "after the second")), shot: Some("after"), ..hold(0.4, "Esc", |i| i.escape_pressed = true) },
        ],
        "aim" => {
            let mut v = vec![Step { setup: Some(aim_setup), ..hold(0.5, "a bow, 10 arrows, a Fluffin 16 m away to the side", |_| {}) }];
            for _ in 0..3 {
                v.push(Step { setup: Some(crosshair_on_creature), ..hold(0.3, "put the crosshair on the Fluffin (from the camera)", |_| {}) });
            }
            for k in 0..3 {
                v.push(Step { setup: Some(crosshair_on_creature), shot: (k == 0).then_some("aim"), ..hold(0.15, "aim", |i| i.aim = true) });
                v.push(hold(1.2, "shoot", |i| {
                    i.aim = true;
                    i.attack_pressed = true;
                }));
            }
            v
        }
        "detour" => {
            // A worker has to get around two beds standing between the Camp Core and the workbench it should work at.
            let mut v = vec![Step { setup: Some(detour_setup), teleport: Some((Vec2::new(16.0, 0.0), None)), yaw: Some(1.57), pitch: Some(-0.45), ..hold(1.0, "Camp Core, 2 beds, a workbench behind them with Capture Orbs queued", |_| {}) }];
            for k in 0..10 {
                v.push(Step { setup: Some(report_worker), shot: (k == 2).then_some("walking"), ..hold(2.0, "the worker heads for the workbench", |_| {}) });
            }
            v.push(Step { setup: Some(report_worker), shot: Some("cooking"), ..hold(0.5, "end", |_| {}) });
            v
        }
        "basetools" => vec![
            Step { setup: Some(tools_setup), teleport: Some((Vec2::new(10.0, -6.0), None)), yaw: Some(std::f32::consts::PI), pitch: Some(-0.45), ..hold(8.0, "Tinkit, Sprigling, 2 Fluffins, a ranch, a workbench with orbs queued, 2 nests, a bed, a repair bench, a Job Board", |_| {}) },
            Step { setup: Some(|w| report_workers(w, "spread")), shot: Some("spread"), ..hold(0.2, "report", |_| {}) },
            Step { setup: Some(|w| prio_all(w, "craft")), ..hold(6.0, "Job Priorities: Crafting 5 -> 1 for everyone (the crafters go to the workbench; Ranching stays 5)", |_| {}) },
            Step { setup: Some(|w| report_workers(w, "crafting 1st")), ..hold(0.2, "report", |_| {}) },
            Step { setup: Some(|w| {
                open_menu(w, |w| w.query_filtered::<Entity, With<crate::base_tools::PriorityBoard>>().iter(w).next().map(crate::menus::Menu::WorkBoard));
            }), shot: Some("preferences"), ..hold(0.6, "Job Board: Job Priorities", |_| {}) },
            Step { setup: Some(|w| {
                for _ in 0..5 {
                    prio_all(w, "craft");
                }
            }), ..hold(0.3, "Crafting back to 5 (1 -> 2 -> 3 -> 4 -> Off -> 5)", |_| {}) },
            Step { setup: Some(|w| click(w, crate::menus::Action::Tab(1))), ..hold(0.3, "Assignments tab", |_| {}) },
            Step { setup: Some(board_fix_one), shot: Some("board"), ..hold(0.6, "pick the workbench, assign the Tinkit to it", |_| {}) },
            hold(0.3, "Esc", |i| i.escape_pressed = true),
            Step { setup: Some(|w| report_workers(w, "board fixed")), ..hold(0.2, "report", |_| {}) },
            Step { setup: Some(lift_ranch_worker), ..hold(0.6, "F on a ranch Fluffin: lift", |_| {}) },
            Step { setup: Some(|w| report_workers(w, "lifted")), shot: Some("carry"), ..hold(0.2, "report", |_| {}) },
            Step { setup: Some(face_fire), ..hold(0.4, "walk up to the campfire (a Fluffin can't kindle: refused)", |_| {}) },
            hold(0.6, "F: put it down at the campfire", |i| i.interact_pressed = true),
            Step { setup: Some(|w| report_workers(w, "at the campfire")), ..hold(0.2, "report", |_| {}) },
            Step { setup: Some(lift_ranch_worker), ..hold(0.6, "lift another", |_| {}) },
            Step { setup: Some(face_bench), ..hold(0.4, "walk up to the workbench", |_| {}) },
            hold(0.6, "F: put it to work at the workbench", |i| i.interact_pressed = true),
            Step { setup: Some(|w| report_workers(w, "put at the workbench")), ..hold(3.0, "report", |_| {}) },
            Step { setup: Some(damage_tools), ..hold(0.3, "wear the axe and the pickaxe down", |_| {}) },
            Step { setup: Some(|w| click(w, crate::menus::Action::Repair(None))), ..hold(0.3, "Repair Bench: Repair All", |_| {}) },
            Step { setup: Some(report_tools), ..hold(0.2, "report", |_| {}) },
            Step { setup: Some(|w| set_hour(w, 23.5)), ..hold(8.0, "night", |_| {}) },
            Step { setup: Some(|w| report_workers(w, "night")), shot: Some("night"), ..hold(0.2, "report", |_| {}) },
            Step { setup: Some(sleep_in_bed), ..hold(0.5, "F at the player's bed", |_| {}) },
            Step { setup: Some(|w| println!("[tools] after sleeping: {:02.1} h", w.resource::<crate::creatures::WorldClock>().hour())), ..hold(0.2, "report", |_| {}) },
        ],
        "settings" => vec![
            Step { setup: Some(base_setup), ..hold(0.5, "start", |_| {}) },
            Step { setup: Some(|w| report_menu(w, "before")), ..hold(0.3, "Esc with nothing open", |i| i.escape_pressed = true) },
            Step { setup: Some(|w| report_menu(w, "after Esc")), shot: Some("menu"), ..hold(0.3, "click Sound On", |_| {}) },
            Step { setup: Some(|w| w.resource_mut::<crate::menus::Clicks>().0.push(crate::menus::Action::Sound(true))), ..hold(0.3, "sound on", |_| {}) },
            Step { setup: Some(|w| report_menu(w, "after Sound On")), ..hold(0.3, "Esc again", |i| i.escape_pressed = true) },
            Step { setup: Some(|w| report_menu(w, "after 2nd Esc")), ..hold(0.3, "start placing a workbench", |_| {}) },
            Step { setup: Some(|w| w.resource_mut::<crate::menus::Clicks>().0.push(crate::menus::Action::Build("workbench".into()))), ..hold(0.5, "placing", |_| {}) },
            Step { setup: Some(|w| report_menu(w, "placing")), ..hold(0.3, "Esc while placing", |i| i.escape_pressed = true) },
            Step { setup: Some(|w| report_menu(w, "after Esc while placing")), ..hold(0.2, "end", |_| {}) },
        ],
        "slope" => vec![
            // Stand still on slopes of about 10, 20 and 30 degrees: the feet must not creep downhill.
            Step { setup: Some(|w| slope_put(w, 10.0)), ..hold(1.5, "on a ~10 deg slope, settle", |_| {}) },
            Step { setup: Some(|w| slope_mark(w, "10 deg")), ..hold(8.0, "stand still 8 s", |_| {}) },
            Step { setup: Some(|w| slope_report(w, "10 deg")), ..hold(0.1, "report", |_| {}) },
            Step { setup: Some(|w| slope_put(w, 20.0)), ..hold(1.5, "on a ~20 deg slope, settle", |_| {}) },
            Step { setup: Some(|w| slope_mark(w, "20 deg")), ..hold(8.0, "stand still 8 s", |_| {}) },
            Step { setup: Some(|w| slope_report(w, "20 deg")), ..hold(0.1, "report", |_| {}) },
            Step { setup: Some(|w| slope_put(w, 30.0)), ..hold(1.5, "on a ~30 deg slope, settle", |_| {}) },
            Step { setup: Some(|w| slope_mark(w, "30 deg")), ..hold(8.0, "stand still 8 s", |_| {}) },
            Step { setup: Some(|w| { slope_report(w, "30 deg"); slope_mark(w, "uphill"); }), ..hold(0.5, "walk uphill", fwd) },
            Step { setup: Some(|w| slope_report(w, "uphill 0.5 s")), ..hold(0.5, "walk uphill", fwd) },
            Step { setup: Some(|w| slope_report(w, "uphill 1.0 s")), ..hold(0.5, "walk uphill", fwd) },
            Step { setup: Some(|w| slope_report(w, "uphill 1.5 s")), ..hold(0.5, "walk uphill", fwd) },
            Step { setup: Some(|w| slope_report(w, "uphill 2.0 s")), ..hold(0.1, "end", |_| {}) },
        ],
        "gear" => vec![
            Step { setup: Some(gear_setup), ..hold(0.5, "bow; only Fire Arrows; Capture / Fine / Grand Orbs; a Feathered Hair Band", |_| {}) },
            Step { setup: Some(|w| report_gear(w, "start")), ..hold(1.2, "aim (auto reload with whatever arrows there are)", |i| i.aim = true) },
            Step { setup: Some(|w| report_gear(w, "after reload")), ..hold(0.3, "shoot", |i| {
                i.aim = true;
                i.attack_pressed = true;
            }) },
            Step { setup: Some(|w| { report_gear(w, "after the Fire Arrow shot"); give_player(w, "arrow", 5); give_player(w, "venom_arrow", 5); }), ..hold(1.2, "now plain and poison arrows too; reload", |i| i.aim = true) },
            Step { setup: Some(|w| report_gear(w, "reloaded")), ..hold(0.4, "hold R: the arrow list opens", |i| i.reload_held = true) },
            Step { setup: Some(|w| report_picker(w, "list open")), shot: Some("arrows"), ..hold(0.2, "wheel down once (R still held)", |i| {
                i.reload_held = true;
                i.weapon_scroll = 1;
            }) },
            Step { setup: Some(|w| report_picker(w, "after the wheel")), ..hold(0.2, "release R", |_| {}) },
            Step { setup: Some(|w| report_gear(w, "after release (Fire -> Poison)")), ..hold(0.4, "hold R again", |i| i.reload_held = true) },
            hold(0.2, "wheel up twice: one step per frame-press", |i| {
                i.reload_held = true;
                i.weapon_scroll = -1;
            }),
            hold(0.2, "wheel up", |i| {
                i.reload_held = true;
                i.weapon_scroll = -1;
            }),
            Step { setup: Some(|w| report_picker(w, "after wheel up x2")), ..hold(0.2, "release", |_| {}) },
            Step { setup: Some(|w| { report_gear(w, "after release (Poison, up twice -> Arrow)"); click_bag_item(w, "fire_arrow"); }), ..hold(0.3, "click Fire Arrow in the bag", |_| {}) },
            Step { setup: Some(|w| report_gear(w, "after clicking Fire Arrow in the bag")), ..hold(0.3, "2: next orb", |i| i.orb_next = true) },
            Step { setup: Some(|w| report_gear(w, "after 2")), ..hold(0.3, "2", |i| i.orb_next = true) },
            Step { setup: Some(|w| report_gear(w, "after 2 again")), ..hold(0.3, "2", |i| i.orb_next = true) },
            Step { setup: Some(|w| { report_gear(w, "after 2 a third time"); equip_item(w, "feather_band"); }), shot: Some("hud"), ..hold(0.3, "equip the hair band", |_| {}) },
            Step { setup: Some(|w| report_gear(w, "equipped")), ..hold(0.3, "inventory", |i| i.menu_pressed = true) },
            Step { setup: Some(|w| report_gear(w, "inventory open")), shot: Some("inventory"), ..hold(0.3, "unequip the hair band", |_| {}) },
            Step { setup: Some(|w| w.resource_mut::<crate::menus::Clicks>().0.push(crate::menus::Action::UnequipHead)), ..hold(0.3, "unequipped", |_| {}) },
            Step { setup: Some(|w| report_gear(w, "after unequip")), ..hold(0.1, "end", |_| {}) },
        ],
        "status" => vec![
            // Fire Arrow on a Fluffin: Burning, slip damage every second, capture chance up; then Poison too.
            Step { setup: Some(status_setup), ..hold(0.6, "bow, Fire and Poison Arrows; a Fluffin 12 m away", |_| {}) },
            Step { setup: Some(|w| { report_target(w, "before"); select_ammo(w, "fire_arrow"); crosshair_on_creature(w); }), ..hold(1.0, "aim (reload a Fire Arrow)", |i| i.aim = true) },
            Step { setup: Some(crosshair_on_creature), ..hold(0.15, "aim", |i| i.aim = true) },
            Step { setup: Some(crosshair_on_creature), ..hold(0.15, "aim", |i| i.aim = true) },
            Step { setup: Some(crosshair_on_creature), ..hold(0.15, "aim", |i| i.aim = true) },
            hold(0.3, "shoot", |i| {
                i.aim = true;
                i.attack_pressed = true;
            }),
            Step { setup: Some(|w| report_target(w, "0.3 s after the Fire Arrow")), shot: Some("burning"), ..hold(3.2, "wait 3 s", |_| {}) },
            Step { setup: Some(|w| { report_target(w, "3.5 s after"); select_ammo(w, "venom_arrow"); crosshair_on_creature(w); }), ..hold(1.0, "aim (reload a Poison Arrow)", |i| i.aim = true) },
            Step { setup: Some(crosshair_on_creature), ..hold(0.15, "aim", |i| i.aim = true) },
            Step { setup: Some(crosshair_on_creature), ..hold(0.15, "aim", |i| i.aim = true) },
            Step { setup: Some(crosshair_on_creature), ..hold(0.15, "aim", |i| i.aim = true) },
            hold(0.3, "shoot", |i| {
                i.aim = true;
                i.attack_pressed = true;
            }),
            Step { setup: Some(|w| report_target(w, "0.3 s after the Poison Arrow")), shot: Some("poisoned"), ..hold(2.2, "wait 2 s", |_| {}) },
            Step { setup: Some(|w| report_target(w, "2.5 s after")), ..hold(16.0, "wait 16 s (Burn ends at 15 s)", |_| {}) },
            Step { setup: Some(|w| report_target(w, "about 20 s after the Fire Arrow")), ..hold(4.0, "wait 4 s (Poison ends at 18 s)", |_| {}) },
            Step { setup: Some(|w| report_target(w, "after both ended")), ..hold(0.1, "end", |_| {}) },
        ],
        "pickup" => vec![
            Step { teleport: Some((Vec2::new(5.0, 5.0), None)), yaw: Some(0.0), pitch: Some(-0.3), ..hold(0.8, "stand", |_| {}) },
            Step { setup: Some(|w| { drop_at(w, "wood", 3, Vec3::new(0.0, 0.0, -1.8)); drop_at(w, "stone", 2, Vec3::new(0.0, 0.0, -7.0)); report_bag_items(w, "dropped Wood 1.8 m and Stone 7 m in front"); }), ..hold(1.5, "wait", |_| {}) },
            Step { setup: Some(|w| report_bag_items(w, "after 1.5 s standing")), ..hold(2.0, "walk toward the Stone", fwd) },
            Step { setup: Some(|w| report_bag_items(w, "after walking")), ..hold(1.0, "stand", |_| {}) },
            Step { setup: Some(|w| report_bag_items(w, "end")), ..hold(0.1, "end", |_| {}) },
        ],
        "steps" => vec![
            Step { teleport: Some((Vec2::new(5.0, 5.0), None)), yaw: Some(0.0), pitch: Some(-0.2), ..hold(1.0, "idle", |_| {}) },
            Step { setup: Some(|w| steps_mark(w, "walk")), ..hold(4.0, "walk 4 s", fwd) },
            Step { setup: Some(|w| steps_report(w, "walk")), ..hold(4.0, "sprint 4 s", |i| {
                fwd(i);
                i.sprint = true;
            }) },
            Step { setup: Some(|w| steps_report(w, "sprint")), ..hold(1.0, "stop", |_| {}) },
            Step { setup: Some(|w| steps_report(w, "stopped")), ..hold(0.1, "end", |_| {}) },
        ],
        "settle" => {
            // Fewer creatures than jobs, all at priority 1: they must pick once and stay (no swapping back and forth).
            let mut v = vec![Step { setup: Some(settle_setup), teleport: Some((Vec2::new(10.0, -6.0), None)), yaw: Some(std::f32::consts::PI), pitch: Some(-0.45), ..hold(1.0, "Tinkit, Sprigling and a Fluffin, 3 workbenches with orders", |_| {}) }];
            for _ in 0..6 {
                v.push(Step { setup: Some(report_switches), ..hold(3.0, "working", |_| {}) });
            }
            v.push(Step { setup: Some(settle_new_bench), ..hold(6.0, "a 4th workbench gets an order (a new task)", |_| {}) });
            v.push(Step { setup: Some(report_switches), shot: Some("settled"), ..hold(6.0, "working", |_| {}) });
            v.push(Step { setup: Some(report_switches), ..hold(0.2, "end", |_| {}) });
            v
        }
        "inspect" => vec![
            Step { setup: Some(capture_setup), pitch: Some(-0.12), ..hold(1.0, "6 m from a Fluffin", |_| {}) },
            Step { setup: Some(face_creature), ..hold(0.3, "F1", |i| i.inspector_pressed = true) },
            Step { setup: Some(face_creature), shot: Some("aiming"), ..hold(0.6, "aim an orb: capture breakdown", |i| i.throw_held = true) },
        ],
        "save" => vec![
            Step { setup: Some(save_setup), teleport: Some((Vec2::new(8.0, 8.0), None)), ..hold(1.0, "a base: Camp Core, workbench (queue), half-built chest, a worker, items", |_| {}) },
            Step { setup: Some(|w| println!("[save] {:?}", crate::save::save_to(w, &save_test_path()))), ..hold(0.5, "save", |_| {}) },
            Step { setup: Some(wipe), teleport: Some((Vec2::new(-30.0, 20.0), None)), ..hold(1.0, "wipe: buildings, bag, owned creatures, level; walk elsewhere", |_| {}) },
            Step { setup: Some(|w| println!("[save] load {:?}", crate::save::load_from(w, &save_test_path()))), ..hold(2.0, "load", |_| {}) },
            Step { shot: Some("loaded"), ..hold(3.0, "after load: the worker is back at work", |_| {}) },
        ],
        "night" => vec![
            Step { setup: Some(|w| set_hour(w, 5.5)), shot: Some("dawn"), ..hold(0.5, "05:30", |_| {}) },
            Step { setup: Some(|w| set_hour(w, 12.0)), shot: Some("noon"), ..hold(0.5, "12:00", |_| {}) },
            Step { setup: Some(|w| set_hour(w, 18.5)), shot: Some("dusk"), ..hold(0.5, "18:30", |_| {}) },
            Step { setup: Some(|w| set_hour(w, 0.5)), shot: Some("midnight"), ..hold(3.0, "00:30: wild Fluffins sleep", |_| {}) },
        ],
        "menus" => vec![
            Step { setup: Some(menus_setup), ..hold(0.5, "items, Lv3 with tech and status points", |_| {}) },
            Step { shot: Some("inventory"), ..hold(0.6, "Tab: inventory", |i| i.menu_pressed = true) },
            Step { setup: Some(|w| click(w, crate::menus::Action::UseBag(1))), ..hold(0.3, "click Berries: eat", |_| {}) },
            Step { setup: Some(|w| click(w, crate::menus::Action::UseBag(0))), ..hold(0.3, "click the axe: equip", |_| {}) },
            Step { setup: Some(|w| click(w, crate::menus::Action::Status(0))), ..hold(0.3, "status point -> Max HP", |_| {}) },
            Step { setup: Some(|w| click(w, crate::menus::Action::Status(0))), ..hold(0.3, "again: no points left", |_| {}) },
            hold(0.3, "Esc closes", |i| i.escape_pressed = true),
            Step { shot: Some("tech"), ..hold(0.6, "T: technology", |i| i.tech_pressed = true) },
            Step { setup: Some(|w| click(w, crate::menus::Action::Unlock("archery".into()))), ..hold(0.3, "unlock Archery (Lv3, 2 pt)", |_| {}) },
            Step { setup: Some(|w| click(w, crate::menus::Action::Unlock("ranch".into()))), ..hold(0.3, "unlock the Ranch (Lv4): refused", |_| {}) },
            hold(0.3, "Esc", |i| i.escape_pressed = true),
            Step { shot: Some("build"), ..hold(0.6, "B: build list", |i| i.build_pressed = true) },
            Step { setup: Some(|w| click(w, crate::menus::Action::Build("workbench".into()))), shot: Some("ghost"), ..hold(0.6, "pick the workbench: ghost", |_| {}) },
            Step { yaw: Some(0.0), pitch: Some(-1.2), shot: Some("ghost_steep"), ..hold(0.4, "aim far: reason shown", |_| {}) },
            hold(0.3, "RMB cancels", |i| i.aim = true),
            Step { setup: Some(|w| {
                let db = w.resource::<crate::data::Db>().clone();
                let mut roster = w.resource_mut::<crate::creatures::Creatures>();
                let id = roster.0.create(&db, "fluffin", 22, &mut ck_core::rng::GameRng::new(5));
                roster.0.take_ownership(id).expect("room");
                w.resource_mut::<crate::menus::MenuState>().menu = crate::menus::Menu::Creature(id);
            }), shot: Some("creature_details"), ..hold(0.6, "Creature details: a Lv22 Fluffin", |_| {}) },
            Step { setup: Some(|w| {
                let p = w.resource::<crate::creatures::Creatures>().0.party()[0].expect("captured");
                let ind = w.resource::<crate::creatures::Creatures>().0.get(p).cloned().expect("creature");
                let spare = ind.learned.iter().find(|l| !ind.equipped.contains(l)).cloned().unwrap_or_default();
                click(w, crate::menus::Action::Equip(p, spare));
            }), ..hold(0.3, "equip a 4th skill: refused", |_| {}) },
            Step { setup: Some(|w| {
                let p = w.resource::<crate::creatures::Creatures>().0.party()[0].expect("captured");
                let first = w.resource::<crate::creatures::Creatures>().0.get(p).map(|i| i.equipped[0].clone()).unwrap_or_default();
                click(w, crate::menus::Action::Equip(p, first));
            }), ..hold(0.3, "unequip the first", |_| {}) },
        ],
        "work" => vec![
            Step { setup: Some(work_setup), teleport: Some((Vec2::new(10.0, -9.0), None)), yaw: Some(std::f32::consts::PI), pitch: Some(-0.5), ..hold(1.0, "camp: one of each creature, smelter + campfire queued, berry patch, stone quarry, lumber yard", |_| {}) },
            Step { shot: Some("start"), ..hold(5.0, "workers pick jobs (the Kindlet goes to the smelter first)", |_| {}) },
            Step { teleport: Some((Vec2::new(12.5, 13.0), None)), yaw: Some(-0.72), pitch: Some(-0.35), shot: Some("closeup_smelter"), ..hold(3.0, "close-up: kindling", |_| {}) },
            Step { teleport: Some((Vec2::new(10.0, -9.0), None)), yaw: Some(std::f32::consts::PI), pitch: Some(-0.5), shot: Some("smelting"), ..hold(20.0, "smelting, cooking", |_| {}) },
            Step { teleport: Some((Vec2::new(0.5, 12.5), None)), yaw: Some(-0.66), pitch: Some(-0.45), shot: Some("closeup_farm"), ..hold(3.0, "close-up: planting", |_| {}) },
            Step { teleport: Some((Vec2::new(19.5, -7.5), None)), yaw: Some(2.48), pitch: Some(-0.4), shot: Some("closeup_logging"), ..hold(3.0, "close-up: chopping", |_| {}) },
            Step { teleport: Some((Vec2::new(10.0, -9.0), None)), yaw: Some(std::f32::consts::PI), pitch: Some(-0.5), shot: Some("spread"), ..hold(34.0, "planting, mining, chopping", |_| {}) },
            Step { shot: Some("later"), ..hold(50.0, "watering, transport", |_| {}) },
            Step { setup: Some(|w| {
                let fire = w.query::<(Entity, &crate::building::Building)>().iter(w).find(|(_, b)| b.id.eq_ignore_ascii_case("campfire")).map(|(e, _)| e);
                let pe = w.query_filtered::<Entity, With<crate::player::Player>>().iter(w).next();
                if let (Some(fire), Some(pe)) = (fire, pe) {
                    let bag = w.get::<crate::inventory::Inventory>(pe).map(|i| i.bag.slots.len()).unwrap_or(0);
                    w.get_mut::<crate::inventory::Inventory>(pe).expect("inv").bag = ck_core::inventory::Container::new(bag);
                    w.insert_resource(crate::crafting::FreeCraft(true));
                    click(w, crate::menus::Action::Queue(fire, "baked_berries".into(), 1));
                }
            }), ..hold(0.5, "free crafting: empty bag, queue Baked Berries anyway", |_| {}) },
            Step { setup: Some(|w| open_menu(w, |w| w.query_filtered::<Entity, With<crate::crafting::Station>>().iter(w).next().map(crate::menus::Menu::Station))), shot: Some("menu_station"), ..hold(0.8, "station menu", |_| {}) },
            Step { setup: Some(|w| open_menu(w, |w| w.query_filtered::<Entity, With<crate::building::BaseCamp>>().iter(w).next().map(crate::menus::Menu::CampCore))), shot: Some("menu_camp_core"), ..hold(0.8, "Camp Core menu", |_| {}) },
            Step { setup: Some(|w| {
                let id = w.resource::<crate::creatures::Creatures>().0.in_place(ck_core::creature::Place::Camp).first().copied();
                if let Some(id) = id {
                    click(w, crate::menus::Action::Details(id));
                }
            }), shot: Some("menu_camp_core_detail"), ..hold(0.8, "Camp Core: a worker selected", |_| {}) },
            Step { setup: Some(|w| open_menu(w, |w| w.query_filtered::<Entity, With<crate::building::Storage>>().iter(w).next().map(crate::menus::Menu::Storage))), shot: Some("menu_chest"), ..hold(0.8, "chest menu", |_| {}) },
        ],
        "pose" => vec![
            Step { setup: Some(pose_setup), pitch: Some(-0.15), ..hold(0.6, "next to a Fluffin, armed", |_| {}) },
            Step { setup: Some(side_view), shot: Some("idle"), ..hold(0.5, "side view, fists", |_| {}) },
            Step { shot: Some("punch"), ..hold(0.2, "punch", |i| i.attack_pressed = true) },
            hold(0.6, "recover", |_| {}),
            Step { shot: Some("punch2"), ..hold(0.24, "punch, other hand (hit frame)", |i| i.attack_pressed = true) },
            Step { setup: Some(|w| select(w, "stone_axe")), ..hold(0.6, "axe", |_| {}) },
            Step { shot: Some("axe_raise"), ..hold(0.11, "chop: raise", |i| i.attack_pressed = true) },
            Step { shot: Some("axe_hit"), ..hold(0.12, "chop: hit", |_| {}) },
            hold(0.6, "recover", |_| {}),
            Step { shot: Some("aim_orb"), ..hold(0.5, "aim an orb", |i| i.throw_held = true) },
            Step { shot: Some("throw"), ..hold(0.1, "throw", |_| {}) },
            Step { setup: Some(|w| select(w, "short_bow")), ..hold(1.2, "bow, reload", |_| {}) },
            Step { shot: Some("bow"), ..hold(0.8, "draw the bow", |i| i.aim = true) },
            Step { setup: Some(|w| select(w, "")), shot: Some("walk"), ..hold(0.7, "walk right", |i| i.move_axis = Vec2::X) },
            Step { shot: Some("run"), ..hold(0.8, "sprint right", |i| {
                i.move_axis = Vec2::X;
                i.sprint = true;
            }) },
        ],
        _ => return None,
    })
}

/// A camp with one creature of each species and one of each production building.
fn work_setup(w: &mut World) {
    base_setup(w);
    let db = w.resource::<crate::data::Db>().clone();
    let camp_core = spawn_built(w, "camp_core", Vec2::new(10.0, 4.0), true);
    w.flush();
    if let Some(mut b) = w.get_mut::<crate::building::BaseCamp>(camp_core) {
        b.level = 5;
    }
    let smelter = spawn_built(w, "smelter", Vec2::new(16.0, 9.0), true);
    let fire = spawn_built(w, "campfire", Vec2::new(16.0, 3.0), true);
    spawn_built(w, "stone_quarry", Vec2::new(3.0, -1.0), true);
    spawn_built(w, "lumber_yard", Vec2::new(16.0, -3.0), true);
    spawn_built(w, "berry_patch", Vec2::new(4.0, 8.0), true);
    spawn_built(w, "storage_chest", Vec2::new(10.0, 12.0), true);
    let feed = spawn_built(w, "feed_box", Vec2::new(7.0, 0.0), true);
    w.flush();
    let mut q = w.query_filtered::<&mut crate::inventory::Inventory, With<Player>>();
    let mut inv = q.single(w).expect("player").clone();
    inv.give(&db, "copper_ore", 10);
    inv.give(&db, "berries", 10);
    for (e, recipe, n) in [(smelter, "copper_ingot", 3), (fire, "baked_berries", 3)] {
        if let Some(mut st) = w.get_mut::<crate::crafting::Station>(e)
            && let Err(why) = crate::crafting::queue(&db, &mut st, &mut inv, recipe, n, false)
        {
            println!("[work] queue {recipe}: {why}");
        }
    }
    *q.single_mut(w).expect("player") = inv;
    if let Some(mut s) = w.get_mut::<crate::building::Storage>(feed) {
        s.items.add("berries", 30, &*db.data);
    }
    let mut rng = ck_core::rng::GameRng::new(9);
    let mut roster = w.resource_mut::<crate::creatures::Creatures>();
    for species in ["kindlet", "sprigling", "mossnut"] {
        let id = roster.0.create(&db, species, 5, &mut rng);
        let _ = roster.0.take_ownership(id);
    }
    let owned: Vec<u64> = roster.0.all.values().filter(|p| p.owned).map(|p| p.id).collect();
    for id in owned {
        let _ = roster.0.move_to(id, ck_core::creature::Place::Camp);
    }
    w.resource_mut::<crate::creatures::WorldClock>().minutes = 8.0 * 60.0;
}

/// Next to a wild Fluffin with an axe, a bow and arrows, orbs and an empty weapon slot (fists).
fn pose_setup(w: &mut World) {
    approach_creature(w);
    let db = w.resource::<crate::data::Db>().clone();
    let mut q = w.query_filtered::<&mut crate::inventory::Inventory, With<Player>>();
    for mut inv in q.iter_mut(w) {
        for id in ["stone_axe", "short_bow"] {
            inv.give(&db, id, 1);
            if let Some(i) = inv.bag.slots.iter().position(|s| s.as_ref().is_some_and(|s| s.item == id)) {
                let _ = inv.equip_from_bag(&db, i);
            }
        }
        inv.give(&db, "arrow", 20);
        inv.give(&db, "capture_orb", 5);
    }
    select(w, "");
}

/// Selects the weapon slot holding `id` ("" = an empty slot: fists).
fn select(w: &mut World, id: &str) {
    let mut q = w.query_filtered::<&mut crate::inventory::Inventory, With<Player>>();
    for mut inv in q.iter_mut(w) {
        if let Some(i) = inv.weapons.iter().position(|s| s.as_ref().map_or(id.is_empty(), |s| s.item == id)) {
            inv.selected = i;
        }
    }
}

/// Swings the camera to the player's right side without turning the player.
fn side_view(w: &mut World) {
    let mut q = w.query_filtered::<&Mover, With<Player>>();
    let yaw = q.iter(w).next().map_or(0.0, |m| m.yaw);
    w.resource_mut::<crate::camera::CameraRig>().yaw = yaw + 1.25;
}

fn save_test_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/shots/save_test.json")
}

fn spawn_built(w: &mut World, id: &str, at: Vec2, finished: bool) -> Entity {
    let db = w.resource::<crate::data::Db>().clone();
    let y = w.resource::<Terrain>().height(at.x, at.y);
    let mut st: bevy::ecs::system::SystemState<(Commands, ResMut<Assets<Mesh>>, ResMut<Assets<StandardMaterial>>)> = bevy::ecs::system::SystemState::new(w);
    let (mut c, mut m, mut mats) = st.get_mut(w).expect("params");
    let e = crate::building::spawn_building(&mut c, &mut m, &mut mats, &db, id, Vec3::new(at.x, y, at.y), Quat::IDENTITY, finished);
    st.apply(w);
    e
}

fn save_setup(w: &mut World) {
    base_setup(w);
    spawn_built(w, "camp_core", Vec2::new(10.0, 4.0), true);
    let bench = spawn_built(w, "workbench", Vec2::new(13.0, 8.0), true);
    let chest = spawn_built(w, "storage_chest", Vec2::new(6.0, 12.0), false);
    if let Some(mut c) = w.get_mut::<crate::building::Construction>(chest) {
        c.work = 400.0;
    }
    let db = w.resource::<crate::data::Db>().clone();
    let mut q = w.query_filtered::<&mut crate::inventory::Inventory, With<Player>>();
    let mut inv = q.single(w).expect("player").clone();
    if let Some(mut st) = w.get_mut::<crate::crafting::Station>(bench) {
        crate::crafting::queue(&db, &mut st, &mut inv, "capture_orb", 2, false).expect("materials");
    }
    *q.single_mut(w).expect("player") = inv;
    let id = w.resource::<crate::creatures::Creatures>().0.party()[0].expect("creature");
    w.resource_mut::<crate::creatures::Creatures>().0.move_to(id, ck_core::creature::Place::Camp).expect("base");
    w.resource_mut::<crate::creatures::WorldClock>().minutes = 14.0 * 60.0;
}

fn wipe(w: &mut World) {
    let mut gone: Vec<Entity> = w.query_filtered::<Entity, With<crate::building::Building>>().iter(w).collect();
    gone.extend(w.query::<(Entity, &crate::creatures::CreatureActor)>().iter(w).filter(|(_, a)| !matches!(a.role, crate::creatures::Role::Wild { .. })).map(|(e, _)| e));
    for e in gone {
        w.despawn(e);
    }
    w.resource_mut::<crate::creatures::Creatures>().0.all.retain(|_, p| !p.owned);
    let mut q = w.query_filtered::<&mut crate::inventory::Inventory, With<Player>>();
    *q.single_mut(w).expect("player") = crate::inventory::Inventory::new();
    w.resource_mut::<crate::progress::Progress>().level = 1;
    w.resource_mut::<crate::creatures::WorldClock>().minutes = 2.0 * 60.0;
}

fn set_hour(w: &mut World, h: f32) {
    w.resource_mut::<crate::creatures::WorldClock>().minutes = h * 60.0;
}

fn open_menu(w: &mut World, pick: fn(&mut World) -> Option<crate::menus::Menu>) {
    if let Some(m) = pick(w) {
        w.resource_mut::<crate::menus::MenuState>().menu = m;
    }
}

fn click(w: &mut World, a: crate::menus::Action) {
    w.resource_mut::<crate::menus::Clicks>().0.push(a);
}

fn menus_setup(w: &mut World) {
    let db = w.resource::<crate::data::Db>().clone();
    let mut q = w.query_filtered::<&mut crate::inventory::Inventory, With<Player>>();
    for mut inv in q.iter_mut(w) {
        for (i, n) in [("stone_axe", 1), ("berries", 5), ("wood", 40), ("stone", 20), ("spark_shard", 5)] {
            inv.give(&db, i, n);
        }
    }
    let mut prog = w.resource_mut::<crate::progress::Progress>();
    prog.level = 3;
    prog.tech_points = 4;
    prog.status_points = 1;
    if let Some(mut h) = w.query_filtered::<&mut crate::player::Hunger, With<Player>>().iter_mut(w).next() {
        h.value = 50.0;
    }
}

fn dismantle_ranch(w: &mut World) {
    let mut q = w.query_filtered::<Entity, With<crate::base::Ranch>>();
    let list: Vec<Entity> = q.iter(w).collect();
    for e in list {
        w.despawn(e);
    }
}

const MODELS_AT: Vec2 = Vec2::new(-20.0, -20.0);
const MODEL_COLS: usize = 6;
const MODEL_GAP: f32 = 4.5;
const MODEL_ROWS: usize = 19;

fn models_setup(w: &mut World) {
    let db = w.resource::<crate::data::Db>().clone();
    let mut ids: Vec<(String, String)> = db.builds.iter().map(|b| (b.category.clone(), b.id.clone())).collect();
    ids.sort();
    for (i, (_, id)) in ids.iter().enumerate() {
        let (c, r) = (i % MODEL_COLS, i / MODEL_COLS);
        let at = Vec2::new(MODELS_AT.x + (c as f32 - (MODEL_COLS as f32 - 1.0) / 2.0) * MODEL_GAP, MODELS_AT.y - r as f32 * MODEL_GAP);
        spawn_built(w, id, at, true);
    }
    println!("[models] {} objects", ids.len());
}

fn dismantle_setup(w: &mut World) {
    let db = w.resource::<crate::data::Db>().clone();
    teleport(w, Vec2::new(30.0, 30.0), None);
    // The crosshair runs 0.6 m right of the player (camera offset): put the objects on that line.
    spawn_built(w, "workbench", Vec2::new(30.6, 26.5), true);
    let chest = spawn_built(w, "storage_chest", Vec2::new(30.6, 22.5), true);
    if let Some(mut s) = w.get_mut::<crate::building::Storage>(chest) {
        s.items.add("wood", 7, &*db.data);
    }
    report_bag(w, "before");
}

fn detour_setup(w: &mut World) {
    base_setup(w);
    let db = w.resource::<crate::data::Db>().clone();
    spawn_built(w, "camp_core", Vec2::new(10.0, 5.0), true);
    spawn_built(w, "bed", Vec2::new(9.5, 0.5), true);
    spawn_built(w, "bed", Vec2::new(10.5, 0.5), true);
    let fire = spawn_built(w, "workbench", Vec2::new(10.0, -3.5), true);
    w.flush();
    let mut q = w.query_filtered::<&mut crate::inventory::Inventory, With<Player>>();
    let mut inv = q.single(w).expect("player").clone();
    if let Some(mut st) = w.get_mut::<crate::crafting::Station>(fire)
        && let Err(why) = crate::crafting::queue(&db, &mut st, &mut inv, "capture_orb", 3, false)
    {
        println!("[detour] queue: {why}");
    }
    *q.single_mut(w).expect("player") = inv;
    let mut roster = w.resource_mut::<crate::creatures::Creatures>();
    let owned: Vec<u64> = roster.0.all.values().filter(|p| p.owned).map(|p| p.id).collect();
    for id in owned {
        let _ = roster.0.move_to(id, ck_core::creature::Place::Camp);
    }
    w.resource_mut::<crate::creatures::WorldClock>().minutes = 8.0 * 60.0;
}

fn report_worker(w: &mut World) {
    let t = w.resource::<Time>().elapsed_secs();
    let mut q = w.query::<(&crate::creatures::CreatureActor, &Mover, &crate::creatures::Brain)>();
    for (a, m, b) in q.iter(w) {
        if a.role == crate::creatures::Role::Worker {
            println!("[detour] t={t:5.1} at ({:5.2}, {:5.2}) {}", m.feet.x, m.feet.z, b.note);
        }
    }
}

fn tools_setup(w: &mut World) {
    base_setup(w);
    let db = w.resource::<crate::data::Db>().clone();
    let camp_core = spawn_built(w, "camp_core", Vec2::new(10.0, 4.0), true);
    w.flush();
    if let Some(mut b) = w.get_mut::<crate::building::BaseCamp>(camp_core) {
        b.level = 5;
    }
    spawn_built(w, "ranch", Vec2::new(-1.0, 12.0), true);
    let bench = spawn_built(w, "workbench", Vec2::new(16.0, 6.0), true);
    spawn_built(w, "campfire", Vec2::new(16.0, 0.0), true);
    spawn_built(w, "creature_bed", Vec2::new(5.0, 0.0), true);
    spawn_built(w, "creature_bed", Vec2::new(6.5, 0.0), true);
    spawn_built(w, "bed", Vec2::new(3.0, -3.0), true);
    spawn_built(w, "repair_bench", Vec2::new(12.0, -3.0), true);
    spawn_built(w, "job_board", Vec2::new(8.0, -3.0), true);
    spawn_built(w, "standing_torch", Vec2::new(13.0, 1.5), true);
    spawn_built(w, "fireplace", Vec2::new(4.0, -6.0), true);
    let feed = spawn_built(w, "feed_box", Vec2::new(7.0, 8.0), true);
    w.flush();
    let mut q = w.query_filtered::<&mut crate::inventory::Inventory, With<Player>>();
    let mut inv = q.single(w).expect("player").clone();
    if let Some(mut st) = w.get_mut::<crate::crafting::Station>(bench)
        && let Err(why) = crate::crafting::queue(&db, &mut st, &mut inv, "capture_orb", 20, false)
    {
        println!("[tools] queue: {why}");
    }
    *q.single_mut(w).expect("player") = inv;
    if let Some(mut s) = w.get_mut::<crate::building::Storage>(feed) {
        s.items.add("berries", 30, &*db.data);
    }
    let mut rng = ck_core::rng::GameRng::new(4);
    let mut roster = w.resource_mut::<crate::creatures::Creatures>();
    for species in ["sprigling", "fluffin"] {
        let id = roster.0.create(&db, species, 5, &mut rng);
        let _ = roster.0.take_ownership(id);
    }
    let owned: Vec<u64> = roster.0.all.values().filter(|p| p.owned).map(|p| p.id).collect();
    for id in owned {
        let _ = roster.0.move_to(id, ck_core::creature::Place::Camp);
    }
    w.resource_mut::<crate::creatures::WorldClock>().minutes = 8.0 * 60.0;
}

fn report_workers(w: &mut World, when: &str) {
    let mut q = w.query::<(&crate::creatures::CreatureActor, &crate::base::WorkerTask, &crate::creatures::Brain)>();
    let mut kinds: Vec<String> = q.iter(w).filter(|(a, ..)| a.role == crate::creatures::Role::Worker).map(|(a, t, b)| format!("#{} {} ({})", a.id, t.kind, b.note)).collect();
    kinds.sort();
    println!("[tools] {when}: {}", kinds.join(" | "));
}

fn settle_setup(w: &mut World) {
    base_setup(w);
    spawn_built(w, "camp_core", Vec2::new(10.0, 4.0), true);
    for x in [4.0, 10.0, 16.0] {
        let e = spawn_built(w, "workbench", Vec2::new(x, -1.0), true);
        w.flush();
        queue_on(w, e, 30);
    }
    let db = w.resource::<crate::data::Db>().clone();
    let mut rng = ck_core::rng::GameRng::new(6);
    let mut roster = w.resource_mut::<crate::creatures::Creatures>();
    let id = roster.0.create(&db, "sprigling", 5, &mut rng);
    let _ = roster.0.take_ownership(id);
    let owned: Vec<u64> = roster.0.all.values().filter(|p| p.owned).map(|p| p.id).collect();
    for id in owned {
        let _ = roster.0.move_to(id, ck_core::creature::Place::Camp);
    }
    w.resource_mut::<crate::creatures::WorldClock>().minutes = 8.0 * 60.0;
}

fn queue_on(w: &mut World, e: Entity, n: u32) {
    let db = w.resource::<crate::data::Db>().clone();
    let mut q = w.query_filtered::<&mut crate::inventory::Inventory, With<Player>>();
    let mut inv = q.single(w).expect("player").clone();
    inv.give(&db, "wood", 200);
    inv.give(&db, "stone", 200);
    inv.give(&db, "spark_shard", 200);
    if let Some(mut st) = w.get_mut::<crate::crafting::Station>(e)
        && let Err(why) = crate::crafting::queue(&db, &mut st, &mut inv, "capture_orb", n, false)
    {
        println!("[settle] queue: {why}");
    }
    *q.single_mut(w).expect("player") = inv;
}

fn settle_new_bench(w: &mut World) {
    let e = spawn_built(w, "workbench", Vec2::new(10.0, 9.0), true);
    w.flush();
    queue_on(w, e, 30);
}

fn report_switches(w: &mut World) {
    let t = w.resource::<Time>().elapsed_secs();
    let mut q = w.query::<(&crate::creatures::CreatureActor, &crate::base::WorkerTask, &Mover)>();
    let mut rows: Vec<String> = q.iter(w).filter(|(a, ..)| a.role == crate::creatures::Role::Worker).map(|(a, tk, m)| format!("#{} {} switches={} at ({:.1},{:.1})", a.id, tk.kind, tk.switches, m.feet.x, m.feet.z)).collect();
    rows.sort();
    println!("[settle] t={t:5.1} {}", rows.join(" | "));
}

fn camp_creature_ids(w: &mut World) -> Vec<u64> {
    w.resource::<crate::creatures::Creatures>().0.in_place(ck_core::creature::Place::Camp)
}

/// A spot near the start whose terrain slope is close to `deg`; the player stands there facing uphill.
fn slope_put(w: &mut World, deg: f32) {
    let t = w.resource::<Terrain>();
    let mut best: Option<(f32, Vec2)> = None;
    for i in -60..=60 {
        for j in -60..=60 {
            let p = Vec2::new(i as f32 * 2.0, j as f32 * 2.0);
            let n = t.normal(p.x, p.y);
            // Even slope around the spot (not a crease), above water.
            let around = [Vec2::X, Vec2::Y, -Vec2::X, -Vec2::Y].iter().map(|d| t.normal(p.x + d.x, p.y + d.y).dot(n)).fold(1.0f32, f32::min);
            if around < 0.995 || t.height(p.x, p.y) < 1.0 {
                continue;
            }
            let err = (n.y.acos().to_degrees() - deg).abs();
            if best.is_none_or(|(e, _)| err < e) {
                best = Some((err, p));
            }
        }
    }
    let Some((_, p)) = best else { return };
    let n = w.resource::<Terrain>().normal(p.x, p.y);
    teleport(w, p, None);
    let up = -Vec3::new(n.x, 0.0, n.z).normalize_or(Vec3::Z);
    w.resource_mut::<crate::camera::CameraRig>().yaw = yaw_of(up);
    println!("[slope] {deg} deg: spot ({:.0},{:.0}) actual {:.1} deg", p.x, p.y, n.y.acos().to_degrees());
}

fn slope_mark(w: &mut World, label: &str) {
    let f = player_feet(w);
    w.resource_mut::<SlopeMark>().0 = Some(f);
    println!("[slope] {label}: start feet ({:.3},{:.3},{:.3})", f.x, f.y, f.z);
}

fn slope_report(w: &mut World, label: &str) {
    let f = player_feet(w);
    let from = w.resource::<SlopeMark>().0.unwrap_or(f);
    println!("[slope] {label}: moved {:.3} m (feet ({:.3},{:.3},{:.3}))", (f - from).length(), f.x, f.y, f.z);
    let mut q = w.query_filtered::<&Mover, With<Player>>();
    if let Some(m) = q.iter(w).next() {
        let t = w.resource::<Terrain>();
        println!("[slope]   grounded={} ground_n={:.3} contacts={:?} terrain_n={:.3} vel={:.2}", m.grounded, m.ground_normal, m.contacts.iter().map(|n| format!("{n:.2}")).collect::<Vec<_>>(), t.normal(f.x, f.z), m.vel);
    }
}

#[derive(Resource, Default)]
pub struct SlopeMark(pub Option<Vec3>);

fn give_player(w: &mut World, item: &str, n: u32) {
    let db = w.resource::<crate::data::Db>().clone();
    let mut q = w.query_filtered::<&mut crate::inventory::Inventory, With<Player>>();
    for mut inv in q.iter_mut(w) {
        inv.give(&db, item, n);
    }
}

fn equip_item(w: &mut World, item: &str) {
    let db = w.resource::<crate::data::Db>().clone();
    let mut q = w.query_filtered::<&mut crate::inventory::Inventory, With<Player>>();
    for mut inv in q.iter_mut(w) {
        if let Some(slot) = inv.bag.slots.iter().position(|s| s.as_ref().is_some_and(|s| s.item == item)) {
            println!("[gear] equip: {:?}", inv.equip_from_bag(&db, slot));
        }
    }
}

fn gear_setup(w: &mut World) {
    {
        let mut q = w.query_filtered::<&mut crate::inventory::Inventory, With<Player>>();
        for mut inv in q.iter_mut(w) {
            inv.weapons[0] = Some(ck_core::inventory::Stack::new("short_bow", 1));
            inv.selected = 0;
        }
    }
    give_player(w, "fire_arrow", 5);
    give_player(w, "capture_orb", 3);
    give_player(w, "fine_orb", 2);
    give_player(w, "grand_orb", 1);
    give_player(w, "feather_band", 1);
}

fn report_picker(w: &mut World, when: &str) {
    let p = w.resource::<crate::inventory::ArrowPicker>();
    println!("[gear] {when}: picker open={} on={}", p.open, crate::inventory::ARROWS[p.sel]);
}

fn click_bag_item(w: &mut World, item: &str) {
    let mut q = w.query_filtered::<&crate::inventory::Inventory, With<Player>>();
    let slot = q.iter(w).next().and_then(|inv| inv.bag.slots.iter().position(|s| s.as_ref().is_some_and(|s| s.item == item)));
    if let Some(slot) = slot {
        w.resource_mut::<crate::menus::Clicks>().0.push(crate::menus::Action::UseBag(slot));
    }
}

fn select_ammo(w: &mut World, item: &str) {
    let db = w.resource::<crate::data::Db>().clone();
    let mut q = w.query_filtered::<&mut crate::inventory::Inventory, With<Player>>();
    for mut inv in q.iter_mut(w) {
        inv.select_ammo(&db, item);
    }
}

fn status_setup(w: &mut World) {
    {
        let mut q = w.query_filtered::<&mut crate::inventory::Inventory, With<Player>>();
        for mut inv in q.iter_mut(w) {
            inv.weapons[0] = Some(ck_core::inventory::Stack::new("short_bow", 1));
            inv.selected = 0;
        }
    }
    give_player(w, "fire_arrow", 5);
    give_player(w, "venom_arrow", 5);
    give_player(w, "capture_orb", 1);
    if let Some((_, p)) = nearest_wild(w, Vec3::new(18.0, 0.0, -18.0)) {
        put_player_near(w, p, 12.0);
    }
}

/// HP, statuses and the Capture Orb capture chance of the nearest wild creature.
fn report_target(w: &mut World, when: &str) {
    let p = player_feet(w);
    let Some((e, _)) = nearest_wild(w, p) else { return };
    let db = w.resource::<crate::data::Db>().clone();
    let Some(a) = w.get::<crate::creatures::CreatureActor>(e).cloned() else { return };
    let Some(h) = w.get::<crate::player::Health>(e).map(|h| (h.hp, h.max)) else { return };
    let st = w.get::<crate::status::Afflicted>(e).cloned();
    let roster = w.resource::<crate::creatures::Creatures>();
    let Some(ind) = roster.0.get(a.id) else { return };
    let with = ck_core::capture::capture_chance(db.rules(), &crate::capture::inputs(&db, "capture_orb", ind, h.0, h.1, false, st.as_ref())).result;
    let without = ck_core::capture::capture_chance(db.rules(), &crate::capture::inputs(&db, "capture_orb", ind, h.0, h.1, false, None)).result;
    let list: Vec<String> = st.map(|s| s.0.iter().map(|r| format!("{} {:.1}s left", r.status.name(&db), r.left)).collect()).unwrap_or_default();
    println!("[status] {when}: {} hp {:.0}/{:.0} [{}] capture {:.1}% (no status {:.1}%)", db.creatures[&ind.species].name, h.0, h.1, list.join(", "), with * 100.0, without * 100.0);
}

fn drop_at(w: &mut World, item: &str, n: u32, ahead: Vec3) {
    let db = w.resource::<crate::data::Db>().clone();
    let a = w.resource::<crate::drops::DropAssets>().clone();
    let at = player_feet(w) + ahead;
    let y = w.resource::<Terrain>().height(at.x, at.z) + 0.5;
    let mut rng = ck_core::rng::GameRng::new(3);
    let mut commands = w.commands();
    crate::drops::spawn_drop(&mut commands, &a, &db, item, n, at.with_y(y), &mut rng);
    w.flush();
}

fn report_bag_items(w: &mut World, when: &str) {
    let mut q = w.query_filtered::<&crate::inventory::Inventory, With<Player>>();
    let (wood, stone) = q.iter(w).next().map_or((0, 0), |inv| (inv.count("wood"), inv.count("stone")));
    let mut dq = w.query::<&crate::drops::WorldItem>();
    let left: Vec<String> = dq.iter(w).map(|d| format!("{} x{}", d.item, d.count)).collect();
    let f = player_feet(w);
    let mut tq = w.query::<(&crate::drops::WorldItem, &Transform)>();
    let at: Vec<String> = tq.iter(w).map(|(d, t)| format!("{} {:.1} m away", d.item, (t.translation - f - Vec3::Y * 0.9).length())).collect();
    println!("[pickup]   player ({:.1},{:.1},{:.1}); {}", f.x, f.y, f.z, at.join(", "));
    println!("[pickup] {when}: bag Wood {wood} Stone {stone}; on the ground: [{}]", left.join(", "));
}

fn report_gear(w: &mut World, when: &str) {
    let mut q = w.query_filtered::<&crate::inventory::Inventory, With<Player>>();
    let Some(inv) = q.iter(w).next() else { return };
    let arrows: Vec<String> = crate::inventory::ARROWS.iter().map(|a| format!("{a} {}", inv.count(a))).collect();
    println!("[gear] {when}: ammo={} loaded={} bag[{}] orb={} head={:?} weight={:.1}", inv.ammo, inv.loaded, arrows.join(", "), inv.orb, inv.head.as_ref().map(|s| s.item.clone()), inv.weight(w.resource::<crate::data::Db>()));
}

fn steps_mark(w: &mut World, label: &str) {
    let f = player_feet(w);
    w.resource_mut::<SlopeMark>().0 = Some(f);
    let n = footfalls(w);
    w.resource_mut::<StepMark>().0 = n;
    let _ = label;
}

fn footfalls(w: &mut World) -> u32 {
    let mut q = w.query::<&crate::anim::PlayerRig>();
    q.iter(w).next().map_or(0, |r| r.footfalls)
}

fn steps_report(w: &mut World, label: &str) {
    let f = player_feet(w);
    let from = w.resource::<SlopeMark>().0.unwrap_or(f);
    let n = footfalls(w);
    let was = w.resource::<StepMark>().0;
    let d = (f - from).with_y(0.0).length();
    println!("[steps] {label}: {} footfalls over {:.1} m ({:.2} m per step)", n - was, d, d / (n - was).max(1) as f32);
    w.resource_mut::<SlopeMark>().0 = Some(f);
    w.resource_mut::<StepMark>().0 = n;
}

#[derive(Resource, Default)]
pub struct StepMark(pub u32);

fn report_menu(w: &mut World, when: &str) {
    let menu = w.resource::<crate::menus::MenuState>().menu;
    let placing = w.resource::<crate::building::Placing>().id.clone();
    let s = w.resource::<crate::settings::Settings>().clone();
    println!("[settings] {when}: menu={menu:?} placing={placing:?} fullscreen={} muted={} sfx_muted={} persist={}", s.fullscreen, s.muted, crate::sfx::is_muted(w), s.persist);
}

fn prio_all(w: &mut World, suit: &str) {
    for id in camp_creature_ids(w) {
        w.resource_mut::<crate::menus::Clicks>().0.push(crate::menus::Action::CreaturePrio(id, suit.to_owned()));
    }
}

fn board_fix_one(w: &mut World) {
    let Some(bench) = bench(w) else { return };
    let Some(id) = camp_creature_ids(w).first().copied() else { return };
    w.resource_mut::<crate::menus::MenuState>().sel_ent = Some(bench);
    w.resource_mut::<crate::menus::MenuState>().dirty = true;
    w.resource_mut::<crate::menus::Clicks>().0.push(crate::menus::Action::FixCreature(id, bench));
}

fn lift_ranch_worker(w: &mut World) {
    let mut q = w.query::<(Entity, &crate::creatures::CreatureActor, &crate::base::WorkerTask)>();
    let pick = q.iter(w).filter(|(_, a, _)| a.role == crate::creatures::Role::Worker).find(|(.., t)| t.kind == "ranch").or_else(|| q.iter(w).find(|(_, a, t)| a.role == crate::creatures::Role::Worker && t.kind != "carried")).map(|(e, ..)| e);
    match pick {
        Some(e) => {
            w.write_message(crate::interact::Interact(e));
        }
        None => println!("[tools] no worker to lift"),
    }
}

fn building_at(w: &mut World, id: &str) -> Option<Vec3> {
    let mut q = w.query::<(&crate::building::Building, &GlobalTransform)>();
    q.iter(w).find(|(b, _)| b.id == id).map(|(_, t)| t.translation())
}

fn face_fire(w: &mut World) {
    if let Some(p) = building_at(w, "campfire") {
        put_player_near(w, p, 1.6);
    }
}

fn face_bench(w: &mut World) {
    if let Some(p) = building_at(w, "workbench") {
        put_player_near(w, p, 1.9);
    }
}

fn damage_tools(w: &mut World) {
    let db = w.resource::<crate::data::Db>().clone();
    let mut q = w.query_filtered::<&mut crate::inventory::Inventory, With<Player>>();
    for mut inv in q.iter_mut(w) {
        for (slot, item) in [(0, "stone_axe"), (1, "stone_pickaxe")] {
            let mut s = ck_core::inventory::Stack::new(item, 1);
            s.durability = db.item(item).and_then(|i| i.durability).map(|d| d * 0.3);
            inv.weapons[slot] = Some(s);
        }
        inv.give(&db, "wood", 20);
        inv.give(&db, "stone", 20);
    }
    report_tools(w);
}

fn report_tools(w: &mut World) {
    let mut q = w.query_filtered::<&crate::inventory::Inventory, With<Player>>();
    if let Some(inv) = q.iter(w).next() {
        let d: Vec<String> = inv.weapons.iter().flatten().map(|s| format!("{} {:?}", s.item, s.durability.map(|d| d.round()))).collect();
        println!("[tools] tools: {} | Wood {} Stone {}", d.join(", "), inv.count("wood"), inv.count("stone"));
    }
}

fn sleep_in_bed(w: &mut World) {
    let mut q = w.query::<(Entity, &crate::base_tools::Bed)>();
    let bed = q.iter(w).find(|(_, b)| !b.creature).map(|(e, _)| e);
    if let Some(e) = bed {
        w.write_message(crate::interact::Interact(e));
    }
}

fn report_bag(w: &mut World, when: &str) {
    let mut q = w.query_filtered::<&crate::inventory::Inventory, With<Player>>();
    let inv = q.iter(w).next().cloned();
    let mut b = w.query::<&crate::building::Building>();
    let left: Vec<String> = b.iter(w).filter(|x| x.id == "workbench" || x.id == "storage_chest").map(|x| x.name.clone()).collect();
    if let Some(inv) = inv {
        println!("[dismantle] {when}: Wood {} Stone {} | still standing: {:?}", inv.count("wood"), inv.count("stone"), left);
    }
}

fn aim_setup(w: &mut World) {
    let db = w.resource::<crate::data::Db>().clone();
    let mut q = w.query_filtered::<&mut crate::inventory::Inventory, With<Player>>();
    for mut inv in q.iter_mut(w) {
        inv.weapons[0] = Some(ck_core::inventory::Stack::new("short_bow", 1));
        inv.selected = 0;
        inv.give(&db, "arrow", 10);
    }
    if let Some((_, p)) = nearest_wild(w, Vec3::new(18.0, 0.0, -18.0)) {
        put_player_near(w, p, 16.0);
    }
}

/// Turns the camera so its centre ray (the crosshair) passes through the nearest wild creature's body.
fn crosshair_on_creature(w: &mut World) {
    let p = player_feet(w);
    let Some((_, t)) = nearest_wild(w, p) else { return };
    let mut q = w.query_filtered::<&GlobalTransform, With<crate::camera::MainCamera>>();
    let Some(eye) = q.iter(w).next().map(|g| g.translation()) else { return };
    let d = (t + Vec3::Y * 0.5 - eye).normalize();
    let mut rig = w.resource_mut::<crate::camera::CameraRig>();
    rig.yaw = yaw_of(d.with_y(0.0));
    rig.pitch = d.y.asin();
}

fn start_placing(w: &mut World, id: &str) {
    let mut p = w.resource_mut::<crate::building::Placing>();
    p.id = Some(id.to_owned());
    p.rot = 0.0;
}

fn base_setup(w: &mut World) {
    // A clear building site: no trees, rocks or ore within 30 m of the camp spot.
    let mut nodes = w.query_filtered::<(Entity, &GlobalTransform), With<crate::resources::ResourceNode>>();
    let near: Vec<Entity> = nodes.iter(w).filter(|(_, t)| t.translation().xz().distance(Vec2::new(10.0, 4.0)) < 30.0).map(|(e, _)| e).collect();
    for e in near {
        w.despawn(e);
    }
    let db = w.resource::<crate::data::Db>().clone();
    let mut q = w.query_filtered::<&mut crate::inventory::Inventory, With<Player>>();
    for mut inv in q.iter_mut(w) {
        for (i, n) in [("wood", 300), ("stone", 200), ("spark_shard", 60), ("fiber", 80), ("berries", 30)] {
            inv.give(&db, i, n);
        }
        inv.weight_points = 20;
    }
    {
        let mut prog = w.resource_mut::<crate::progress::Progress>();
        prog.level = 5;
        for t in db.technologies.iter().filter(|t| t.level <= 5) {
            prog.unlocked.insert(t.id.clone());
        }
    }
    let mut rng = ck_core::rng::GameRng::new(8);
    let mut roster = w.resource_mut::<crate::creatures::Creatures>();
    for species in ["tinkit", "fluffin"] {
        let id = roster.0.create(&db, species, 5, &mut rng);
        roster.0.take_ownership(id).expect("room");
    }
}

fn assign_worker(w: &mut World) {
    let party = w.resource::<crate::creatures::Creatures>().0.party();
    for id in party.into_iter().flatten() {
        w.resource_mut::<crate::menus::Clicks>().0.push(crate::menus::Action::ToCamp(id));
    }
}

fn bench(w: &mut World) -> Option<Entity> {
    let mut q = w.query::<(Entity, &crate::building::Building, Has<crate::crafting::Station>)>();
    q.iter(w).find(|(_, b, st)| b.id == "workbench" && *st).map(|(e, ..)| e)
}

fn queue_orbs(w: &mut World) {
    if let Some(e) = bench(w) {
        w.resource_mut::<crate::menus::Clicks>().0.push(crate::menus::Action::Queue(e, "capture_orb".into(), 3));
    } else {
        println!("[base] no finished workbench!");
    }
}

fn collect_bench(w: &mut World) {
    if let Some(e) = bench(w) {
        w.resource_mut::<crate::menus::Clicks>().0.push(crate::menus::Action::Collect(e));
    }
}

fn finish_all(w: &mut World) {
    let db = w.resource::<crate::data::Db>().clone();
    let mut q = w.query::<(Entity, &crate::building::Building, &crate::building::Construction)>();
    let list: Vec<(Entity, String, String)> = q.iter(w).map(|(e, b, _)| (e, b.id.to_ascii_lowercase(), b.name.clone())).collect();
    for (e, id, name) in list {
        let mut c = w.commands();
        crate::building::finish(&mut c, &db, e, &id, &name);
    }
    w.flush();
}

fn nearest_wild(w: &mut World, from: Vec3) -> Option<(Entity, Vec3)> {
    let mut q = w.query::<(Entity, &crate::creatures::CreatureActor, &Mover)>();
    q.iter(w).filter(|(_, a, _)| matches!(a.role, crate::creatures::Role::Wild { .. })).map(|(e, _, m)| (e, m.feet)).min_by(|a, b| a.1.distance(from).total_cmp(&b.1.distance(from)))
}

fn player_feet(w: &mut World) -> Vec3 {
    let mut q = w.query_filtered::<&Mover, With<Player>>();
    q.iter(w).next().map_or(Vec3::ZERO, |m| m.feet)
}

fn put_player_near(w: &mut World, target: Vec3, dist: f32) {
    let p = player_feet(w);
    let dir = (p - target).with_y(0.0).normalize_or(Vec3::X);
    let at = target + dir * dist;
    teleport(w, Vec2::new(at.x, at.z), None);
    face(w, target);
}

fn face(w: &mut World, target: Vec3) {
    let p = player_feet(w);
    let yaw = yaw_of((target - p).with_y(0.0));
    w.resource_mut::<crate::camera::CameraRig>().yaw = yaw;
    let mut q = w.query_filtered::<&mut Mover, With<Player>>();
    for mut m in q.iter_mut(w) {
        m.yaw = yaw;
    }
}

fn approach_creature(w: &mut World) {
    if let Some((_, p)) = nearest_wild(w, Vec3::new(18.0, 0.0, -18.0)) {
        put_player_near(w, p, 1.3);
    }
}

fn approach_creature_far(w: &mut World) {
    if let Some((_, p)) = nearest_wild(w, Vec3::new(18.0, 0.0, -18.0)) {
        put_player_near(w, p, 9.0);
    }
}

fn face_creature(w: &mut World) {
    let p = player_feet(w);
    if let Some((_, t)) = nearest_wild(w, p) {
        face(w, t);
        let d = (t - p).length();
        w.resource_mut::<crate::camera::CameraRig>().pitch = -(1.5f32 - 0.4).atan2(d) * 0.9;
    }
}

fn capture_setup(w: &mut World) {
    let db = w.resource::<crate::data::Db>().clone();
    let mut q = w.query_filtered::<&mut crate::inventory::Inventory, With<Player>>();
    for mut inv in q.iter_mut(w) {
        inv.give(&db, "capture_orb", 5);
    }
    if let Some((_, p)) = nearest_wild(w, Vec3::new(18.0, 0.0, -18.0)) {
        put_player_near(w, p, 6.0);
    }
}

fn companion_setup(w: &mut World) {
    let db = w.resource::<crate::data::Db>().clone();
    let mut rng = ck_core::rng::GameRng::new(5);
    let mut roster = w.resource_mut::<crate::creatures::Creatures>();
    let id = roster.0.create(&db, "fluffin", 8, &mut rng);
    roster.0.take_ownership(id).expect("party has room");
}

fn gather_setup(w: &mut World) {
    let db = w.resource::<crate::data::Db>().clone();
    let mut q = w.query_filtered::<&mut crate::inventory::Inventory, With<Player>>();
    for mut inv in q.iter_mut(w) {
        inv.give(&db, "stone_axe", 1);
        inv.give(&db, "stone_pickaxe", 1);
        let _ = inv.equip_from_bag(&db, 0);
        let _ = inv.equip_from_bag(&db, 1);
        inv.selected = 0;
    }
    go_node(w, crate::resources::NodeKind::Tree);
}

fn node_pos(w: &mut World, kind: crate::resources::NodeKind) -> Option<Vec3> {
    let from = player_feet(w);
    let mut q = w.query::<(&crate::resources::ResourceNode, &GlobalTransform)>();
    q.iter(w).filter(|(n, _)| n.kind == kind && n.alive()).map(|(_, t)| t.translation()).min_by(|a, b| a.distance(from).total_cmp(&b.distance(from)))
}

fn go_node(w: &mut World, kind: crate::resources::NodeKind) {
    if let Some(p) = node_pos(w, kind) {
        let r = if kind == crate::resources::NodeKind::Tree { 1.2 } else { 2.2 };
        put_player_near(w, p, r);
    }
}

fn face_node_tree(w: &mut World) {
    if let Some(p) = node_pos(w, crate::resources::NodeKind::Tree) {
        face(w, p);
    }
}

fn face_node_rock(w: &mut World) {
    if let Some(p) = node_pos(w, crate::resources::NodeKind::Rock) {
        face(w, p);
    }
}

fn go_rock(w: &mut World) {
    go_node(w, crate::resources::NodeKind::Rock);
}

fn go_bush(w: &mut World) {
    go_node(w, crate::resources::NodeKind::BerryBush);
}

fn give_glider(w: &mut World) {
    let mut q = w.query_filtered::<&mut Locomotion, With<Player>>();
    for mut l in q.iter_mut(w) {
        l.has_glider_debug = true;
    }
}

fn heal(w: &mut World) {
    let mut q = w.query_filtered::<&mut Health, With<Player>>();
    for mut h in q.iter_mut(w) {
        h.hp = h.max;
    }
}

pub fn teleport(w: &mut World, p: Vec2, y: Option<f32>) {
    let ground = w.resource::<Terrain>().height(p.x, p.y);
    let mut q = w.query_filtered::<(&mut Mover, &mut Locomotion), With<Player>>();
    for (mut m, mut l) in q.iter_mut(w) {
        m.feet = Vec3::new(p.x, y.map_or(ground + 0.05, |y| ground + y), p.y);
        m.vel = Vec3::ZERO;
        m.grounded = false;
        l.mode = Mode::Air;
        l.fall_origin = m.feet.y;
    }
}

fn world_report(w: &mut World) -> String {
    let p = player_feet(w);
    let mut out = Vec::new();
    let mut q = w.query::<(&crate::creatures::CreatureActor, &Mover, &crate::creatures::Brain, &Health, &crate::combat::Combatant)>();
    let mut near: Vec<_> = q.iter(w).filter(|(_, m, ..)| m.feet.distance(p) < 25.0).map(|(a, m, b, h, c)| (m.feet.distance(p), format!("    creature {} {:?} {} hp={:.0}/{:.0} d={:.1} note={}", c.name, a.role, b.state.name(), h.hp, h.max, m.feet.distance(p), b.note))).collect();
    near.sort_by(|a, b| a.0.total_cmp(&b.0));
    out.extend(near.into_iter().take(4).map(|(_, s)| s));
    let mut cq = w.query_filtered::<(Entity, &GlobalTransform, Option<&Name>), With<avian3d::prelude::Collider>>();
    let mut cols: Vec<(f32, String)> = cq.iter(w).filter(|(_, t, n)| t.translation().with_y(0.0).distance(p.with_y(0.0)) < 3.0 && n.is_none_or(|n| n.as_str() != "Terrain")).map(|(e, t, n)| (t.translation().distance(p), format!("{} {e} at {:.1?}", n.map_or("?", |n| n.as_str()), t.translation()))).collect();
    cols.sort_by(|a, b| a.0.total_cmp(&b.0));
    if !cols.is_empty() {
        out.push(format!("    colliders near: {}", cols.into_iter().take(4).map(|c| c.1).collect::<Vec<_>>().join("; ")));
    }
    let roster = w.resource::<crate::creatures::Creatures>();
    let party: Vec<String> = roster.0.party().iter().map(|s| s.and_then(|id| roster.0.get(id)).map_or("-".into(), |i| format!("{}Lv{}", i.species, i.level))).collect();
    let mut bq = w.query::<(&crate::building::Building, Option<&crate::building::Construction>, Option<&crate::crafting::Station>, Option<&crate::building::Storage>, Option<&crate::building::BaseCamp>, (Option<&crate::work::Producer>, Option<&crate::work::Farm>))>();
    for (b, c, st, sto, bc, (pr, fm)) in bq.iter(w) {
        let mut s = format!("    building {}", b.name);
        if let Some(c) = c {
            s += &format!(" (blueprint {:.0}/{:.0})", c.work, c.need);
        }
        if let Some(st) = st {
            s += &format!(" queue {:?} output {:?}", st.queue.iter().map(|j| (j.recipe.clone(), j.remaining, j.work as u32)).collect::<Vec<_>>(), st.output);
        }
        if let Some(sto) = sto {
            s += &format!(" holds {:?}", sto.items.slots.iter().flatten().map(|x| (x.item.clone(), x.count)).collect::<Vec<_>>());
        }
        if let Some(bc) = bc {
            s += &format!(" base level {}", bc.level);
        }
        if let Some(p) = pr {
            s += &format!(" produce {} {:.0}/{:.0}", p.product, p.work, p.need);
        }
        if let Some(f) = fm {
            s += &format!(" farm {:?} work {:.0} grow {:.0}", f.stage, f.work, f.grow);
        }
        out.push(s);
    }
    let mut wq = w.query::<(&crate::creatures::CreatureActor, &crate::creatures::Brain, &crate::base::WorkerTask)>();
    for (a, b, t) in wq.iter(w) {
        out.push(format!("    worker #{} {} task={} note={}", a.id, b.state.name(), t.kind, b.note));
    }
    let roster = w.resource::<crate::creatures::Creatures>();
    for p in roster.0.all.values().filter(|p| p.owned) {
        out.push(format!("    owned #{} {:?} mood {:.1} food {:.1} hp {:.0}", p.id, p.place, p.mood, p.fullness, p.hp));
    }
    let ms = w.resource::<crate::menus::MenuState>();
    if ms.menu != crate::menus::Menu::None || !ms.msg.is_empty() {
        out.push(format!("    menu {:?} msg {:?}", ms.menu, ms.msg));
    }
    let prog = w.resource::<crate::progress::Progress>();
    out.push(format!("    party [{}] player Lv{} xp {} tech {}", party.join(", "), prog.level, prog.exp, prog.tech_points));
    let mut iq = w.query_filtered::<&crate::inventory::Inventory, With<Player>>();
    if let Some(inv) = iq.iter(w).next() {
        let items: Vec<String> = inv.bag.slots.iter().flatten().map(|s| format!("{}x{}", s.item, s.count)).collect();
        let weapon = inv.weapon().map_or("fists".into(), |s| format!("{}({:.0})", s.item, s.durability.unwrap_or(0.0)));
        out.push(format!("    bag [{}] weapon {weapon}", items.join(" ")));
    }
    out.join("\n")
}

pub fn report(w: &mut World, script: &str, label: &str) {
    let mut q = w.query_filtered::<(&Mover, &Locomotion, &Stamina, &Health), With<Player>>();
    let Some((m, l, s, h)) = q.iter(w).next() else { return };
    let line = format!(
        "[{script}] {label:<26} mode={:<7} crouch={} sprint={} grounded={} hspeed={:.2} vy={:.2} feet=({:.2},{:.2},{:.2}) sp={:.1}{} hp={:.0} fall_dmg={:.0}",
        l.mode.name(),
        l.crouched as u8,
        l.sprinting as u8,
        m.grounded as u8,
        m.vel.with_y(0.0).length(),
        m.vel.y,
        m.feet.x,
        m.feet.y,
        m.feet.z,
        s.sp,
        if s.exhausted { "(ex)" } else { "" },
        h.hp,
        l.last_fall_damage
    );
    println!("{line}");
    let extra = world_report(w);
    if !extra.is_empty() {
        println!("{extra}");
    }
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/shots").join(format!("{script}.log"));
    let _ = std::fs::create_dir_all(path.parent().unwrap_or(std::path::Path::new(".")));
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{line}");
    }
}
