//! Whether the pet should be on screen right now.
//!
//! The decision is here, away from any operating system, so it can be tested; the shell only says
//! whether another app is full screen (owner 2026-10-28: "my pet doesn\'t disappear on full
//! screen").

/// `visible`: the `pet.visible` setting. `hide_when_fullscreen`: the `pet.hide_fullscreen`
/// setting. `fullscreen`: another app is filling a screen right now.
pub fn pet_on_screen(visible: bool, hide_when_fullscreen: bool, fullscreen: bool) -> bool {
    visible && !(hide_when_fullscreen && fullscreen)
}

/// A window counts as full screen when it covers its whole screen. A pixel of slack each side
/// absorbs rounding between the window and the screen it is measured against.
pub fn covers_screen(window: (i32, i32, i32, i32), screen: (i32, i32, i32, i32)) -> bool {
    let (wx, wy, ww, wh) = window;
    let (sx, sy, sw, sh) = screen;
    if ww <= 0 || wh <= 0 || sw <= 0 || sh <= 0 {
        return false;
    }
    wx <= sx + 1 && wy <= sy + 1 && wx + ww >= sx + sw - 1 && wy + wh >= sy + sh - 1
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: (i32, i32, i32, i32) = (0, 0, 1920, 1080);

    #[test]
    fn the_pet_steps_aside_only_while_something_else_fills_the_screen() {
        assert!(
            pet_on_screen(true, true, false),
            "nothing full screen: shown"
        );
        assert!(!pet_on_screen(true, true, true), "a film is on: hidden");
        assert!(
            pet_on_screen(true, false, true),
            "the setting is off: stays"
        );
        assert!(
            !pet_on_screen(false, true, false),
            "hidden by the user stays hidden"
        );
        assert!(!pet_on_screen(false, false, true));
    }

    #[test]
    fn a_window_is_full_screen_only_when_it_covers_the_whole_screen() {
        assert!(covers_screen((0, 0, 1920, 1080), SCREEN));
        assert!(
            covers_screen((-1, -1, 1922, 1082), SCREEN),
            "a hair larger still counts"
        );
        assert!(
            !covers_screen((0, 0, 1920, 1040), SCREEN),
            "the taskbar still shows"
        );
        assert!(!covers_screen((100, 100, 800, 600), SCREEN));
        assert!(!covers_screen((0, 0, 0, 0), SCREEN), "no window at all");
        assert!(
            !covers_screen((0, 0, 1920, 1080), (0, 0, 0, 0)),
            "no screen to measure against"
        );
    }

    #[test]
    fn a_second_screen_is_measured_against_its_own_corner() {
        let right = (1920, 0, 2560, 1440);
        assert!(covers_screen((1920, 0, 2560, 1440), right));
        assert!(
            !covers_screen((0, 0, 1920, 1080), right),
            "full screen on the other one"
        );
    }
}
