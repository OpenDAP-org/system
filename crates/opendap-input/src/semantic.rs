//! Roles and context to semantic actions (§3.4): what most handlers want, whatever produced it.
//! The shell maps the T1's controls with it; other faceplates' controls map by the same rules.

/// A control's role, as the device description names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Action,
    VolumeUp,
    VolumeDown,
    Main,
}

/// What a control event means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Semantic {
    Activate,
    Primary,
    Next,
    Previous,
    Increase,
    Decrease,
    VolumeUp,
    VolumeDown,
    PlayPause,
    NextTrack,
    PreviousTrack,
    /// The system's (Main): never the activity's.
    System,
    /// Nothing (a long Action press with no action bound).
    None,
}

/// The context that decides a control's meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Context {
    pub display_on: bool,
    /// Sound is playing, or the activity asks for the volume.
    pub volume_wanted: bool,
    /// The focused element is locked for adjustment (a slider).
    pub locked: bool,
    /// The focus ring shows (the buttons or a keyboard moved it).
    pub focus_visible: bool,
    /// The screen declares a primary action.
    pub has_primary: bool,
}

/// A short press of `role` in `ctx`.
pub fn press(role: Role, ctx: Context) -> Semantic {
    match role {
        Role::Main => Semantic::System,
        Role::Action if !ctx.display_on => Semantic::PlayPause,
        // Action follows the focus: the primary action by default (the focus lands there), the
        // chosen element once the focus shows; hidden with no primary, it only shows the focus
        // (the activity's components do that with Activate).
        Role::Action if !ctx.focus_visible && ctx.has_primary => Semantic::Primary,
        Role::Action => Semantic::Activate,
        Role::VolumeUp if !ctx.display_on || ctx.volume_wanted => Semantic::VolumeUp,
        Role::VolumeDown if !ctx.display_on || ctx.volume_wanted => Semantic::VolumeDown,
        Role::VolumeUp if ctx.locked => Semantic::Increase,
        Role::VolumeDown if ctx.locked => Semantic::Decrease,
        Role::VolumeUp => Semantic::Previous,
        Role::VolumeDown => Semantic::Next,
    }
}

/// A long press of `role` in `ctx`: the track while the volume buttons are the volume; in focus
/// navigation they repeat instead (controls spec §1.3), and Action's long press is the user's
/// choice, none by default.
pub fn long_press(role: Role, ctx: Context) -> Semantic {
    match role {
        Role::Main => Semantic::System,
        Role::VolumeUp if !ctx.display_on || ctx.volume_wanted => Semantic::NextTrack,
        Role::VolumeDown if !ctx.display_on || ctx.volume_wanted => Semantic::PreviousTrack,
        Role::VolumeUp | Role::VolumeDown => press(role, ctx),
        Role::Action => Semantic::None,
    }
}

/// Whether a held `role` repeats in `ctx` (§5.1: only where the meaning repeats).
pub fn repeats(role: Role, ctx: Context) -> bool {
    matches!(
        press(role, ctx),
        Semantic::Next | Semantic::Previous | Semantic::Increase | Semantic::Decrease
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn on() -> Context {
        Context { display_on: true, ..Default::default() }
    }

    #[test]
    fn volume_is_the_volume_or_the_focus_or_the_value() {
        assert_eq!(press(Role::VolumeDown, on()), Semantic::Next);
        assert_eq!(press(Role::VolumeUp, on()), Semantic::Previous);
        let locked = Context { locked: true, ..on() };
        assert_eq!(press(Role::VolumeUp, locked), Semantic::Increase);
        let playing = Context { volume_wanted: true, ..locked };
        assert_eq!(press(Role::VolumeUp, playing), Semantic::VolumeUp);
        assert_eq!(press(Role::VolumeDown, Context::default()), Semantic::VolumeDown, "display off");
    }

    #[test]
    fn action_follows_the_focus() {
        let primary = Context { has_primary: true, ..on() };
        assert_eq!(press(Role::Action, primary), Semantic::Primary);
        assert_eq!(press(Role::Action, Context { focus_visible: true, ..primary }), Semantic::Activate);
        assert_eq!(press(Role::Action, on()), Semantic::Activate);
        assert_eq!(press(Role::Action, Context::default()), Semantic::PlayPause, "display off");
    }

    #[test]
    fn a_held_volume_button_has_one_meaning() {
        let playing = Context { volume_wanted: true, ..on() };
        assert_eq!(long_press(Role::VolumeUp, playing), Semantic::NextTrack);
        assert!(!repeats(Role::VolumeUp, playing));
        assert!(repeats(Role::VolumeUp, on()));
        assert_eq!(long_press(Role::Action, on()), Semantic::None);
        assert!(!repeats(Role::Action, on()), "Action never repeats");
    }
}
