//! Player input as one struct per frame, so scripted checks (`--auto`) can drive the game exactly like
//! the keyboard. The default key bindings are in `gather`.

use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};

/// This frame's intent. `*_pressed` fields are edges (true for one frame).
#[derive(Resource, Debug, Default, Clone)]
pub struct PlayerInput {
    /// x = right, y = forward, each -1..1.
    pub move_axis: Vec2,
    /// Mouse delta in radians (yaw, pitch).
    pub look: Vec2,
    pub sprint: bool,
    pub jump_pressed: bool,
    pub jump_held: bool,
    pub crouch_pressed: bool,
    pub roll_pressed: bool,
    pub aim: bool,
    pub attack_pressed: bool,
    pub attack_held: bool,
    pub interact_pressed: bool,
    pub interact_held: bool,
    /// F released after a short press (opens a station's menu; holding F works it instead).
    pub interact_tapped: bool,
    pub throw_pressed: bool,
    pub throw_held: bool,
    pub throw_released: bool,
    pub summon_pressed: bool,
    pub partner_pressed: bool,
    pub reload_pressed: bool,
    /// R held (a long press changes the arrow type).
    pub reload_held: bool,
    pub weapon_scroll: i32,
    pub party_next: bool,
    pub party_prev: bool,
    pub orb_next: bool,
    pub command_pressed: bool,
    pub direct_attack_pressed: bool,
    pub menu_pressed: bool,
    pub build_pressed: bool,
    pub tech_pressed: bool,
    pub inspector_pressed: bool,
    pub escape_pressed: bool,
    pub rotate_pressed: bool,
    pub save_pressed: bool,
    pub load_pressed: bool,
    /// F2: toggle free mode (crafting, building and technology cost nothing).
    pub cheat_pressed: bool,
    /// F3: sound on/off.
    pub mute_pressed: bool,
    /// X: dismantle mode on/off.
    pub dismantle_pressed: bool,
}

/// Who fills `PlayerInput`: the keyboard, or a script (devtools).
#[derive(Resource, Debug, Default, PartialEq, Eq)]
pub enum InputSource {
    #[default]
    Human,
    Script,
}

/// A menu is open: the cursor is free and movement/attack input is dropped.
#[derive(Resource, Debug, Default)]
pub struct UiFocus(pub bool);

pub const MOUSE_SENSITIVITY: f32 = 0.0025;

pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerInput>().init_resource::<InputSource>().init_resource::<UiFocus>().add_systems(PreUpdate, gather.after(bevy::input::InputSystems)).add_systems(Update, grab_cursor);
    }
}

/// A press of F shorter than this is a tap (opens a menu); a longer hold works the station instead.
pub const TAP_S: f32 = 0.3;

#[allow(clippy::too_many_arguments)]
fn gather(time: Res<Time>, source: Res<InputSource>, keys: Res<ButtonInput<KeyCode>>, mouse: Res<ButtonInput<MouseButton>>, motion: Res<AccumulatedMouseMotion>, scroll: Res<AccumulatedMouseScroll>, focus: Res<UiFocus>, mut input: ResMut<PlayerInput>, mut f_held: Local<f32>) {
    if *source != InputSource::Human {
        return;
    }
    let tapped = keys.just_released(KeyCode::KeyF) && *f_held < TAP_S;
    *f_held = if keys.pressed(KeyCode::KeyF) { *f_held + time.delta_secs() } else { 0.0 };
    let k = |c: KeyCode| keys.pressed(c);
    let kp = |c: KeyCode| keys.just_pressed(c);
    let mut i = PlayerInput {
        menu_pressed: kp(KeyCode::Tab),
        build_pressed: kp(KeyCode::KeyB),
        tech_pressed: kp(KeyCode::KeyT),
        inspector_pressed: kp(KeyCode::F1),
        cheat_pressed: kp(KeyCode::F2),
        mute_pressed: kp(KeyCode::F3),
        dismantle_pressed: kp(KeyCode::KeyX),
        escape_pressed: kp(KeyCode::Escape) || kp(KeyCode::Backspace),
        save_pressed: kp(KeyCode::F5),
        load_pressed: kp(KeyCode::F9),
        ..default()
    };
    if !focus.0 {
        i.move_axis = Vec2::new((k(KeyCode::KeyD) as i32 - k(KeyCode::KeyA) as i32) as f32, (k(KeyCode::KeyW) as i32 - k(KeyCode::KeyS) as i32) as f32);
        i.look = motion.delta * MOUSE_SENSITIVITY;
        i.sprint = k(KeyCode::ShiftLeft);
        i.jump_pressed = kp(KeyCode::Space);
        i.jump_held = k(KeyCode::Space);
        i.crouch_pressed = kp(KeyCode::KeyC);
        i.roll_pressed = kp(KeyCode::ControlLeft);
        i.aim = mouse.pressed(MouseButton::Right);
        i.attack_pressed = mouse.just_pressed(MouseButton::Left);
        i.attack_held = mouse.pressed(MouseButton::Left);
        i.interact_pressed = kp(KeyCode::KeyF);
        i.interact_held = k(KeyCode::KeyF);
        i.interact_tapped = tapped;
        i.partner_pressed = kp(KeyCode::KeyF);
        i.throw_pressed = kp(KeyCode::KeyQ);
        i.throw_held = k(KeyCode::KeyQ);
        i.throw_released = keys.just_released(KeyCode::KeyQ);
        i.summon_pressed = kp(KeyCode::KeyE);
        i.reload_pressed = kp(KeyCode::KeyR);
        i.reload_held = k(KeyCode::KeyR);
        i.rotate_pressed = kp(KeyCode::KeyR);
        i.weapon_scroll = if scroll.delta.y < 0.0 { 1 } else if scroll.delta.y > 0.0 { -1 } else { 0 };
        i.party_next = kp(KeyCode::Digit3);
        i.party_prev = kp(KeyCode::Digit1);
        i.orb_next = kp(KeyCode::Digit2);
        i.command_pressed = kp(KeyCode::Digit4);
        i.direct_attack_pressed = mouse.just_pressed(MouseButton::Middle);
    }
    *input = i;
}

/// Mouse-look grabs the cursor while no menu is open and the window has focus.
fn grab_cursor(focus: Res<UiFocus>, source: Res<InputSource>, mut q: Query<(&Window, &mut CursorOptions), With<PrimaryWindow>>) {
    let Ok((window, mut c)) = q.single_mut() else { return };
    let want = !focus.0 && window.focused && *source == InputSource::Human;
    let mode = if want { CursorGrabMode::Locked } else { CursorGrabMode::None };
    if c.grab_mode != mode {
        c.grab_mode = mode;
        c.visible = !want;
    }
}
