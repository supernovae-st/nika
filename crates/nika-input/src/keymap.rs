// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Kernel→enigo key mapping — **pure** (headless-testable · mutation-killed).
//!
//! All chord DECISION logic lives here: which enigo key a kernel [`KeyCode`]
//! maps to (fail-closed on unknown variants — the kernel enum is
//! `#[non_exhaustive]`), which modifiers to hold and in what canonical order,
//! and the fully-resolved [`ChordPlan`]. The EXECUTION (a live `&mut Enigo`
//! handle) stays in `lib.rs` (`press_chord` · FFI residue).

use enigo::Key;
use nika_kernel::io::input::{InputError, KeyCode, KeyMods};

/// Map the kernel's 66-variant [`KeyCode`] to an enigo [`Key`].
///
/// Letters + digits go through `Key::Unicode` (lowercase base character — the
/// OS applies Shift from held modifiers). Left/right modifier distinction is
/// COLLAPSED to enigo's generic ungated keys (`Shift`/`Control`/`Alt`/`Meta`):
/// the sided variants are platform-gated in enigo 0.6 (verified on the
/// tarball), so collapsing is what keeps this map cross-platform with zero
/// `#[cfg]` in this crate.
///
/// # Errors
/// [`InputError::EventPostFailed`] for a kernel `KeyCode` variant newer than
/// this crate (the kernel enum is `#[non_exhaustive]` — fail closed, never
/// guess a key).
pub(crate) fn map_key(key: KeyCode) -> Result<Key, InputError> {
    use KeyCode as K;
    Ok(match key {
        K::A => Key::Unicode('a'),
        K::B => Key::Unicode('b'),
        K::C => Key::Unicode('c'),
        K::D => Key::Unicode('d'),
        K::E => Key::Unicode('e'),
        K::F => Key::Unicode('f'),
        K::G => Key::Unicode('g'),
        K::H => Key::Unicode('h'),
        K::I => Key::Unicode('i'),
        K::J => Key::Unicode('j'),
        K::K => Key::Unicode('k'),
        K::L => Key::Unicode('l'),
        K::M => Key::Unicode('m'),
        K::N => Key::Unicode('n'),
        K::O => Key::Unicode('o'),
        K::P => Key::Unicode('p'),
        K::Q => Key::Unicode('q'),
        K::R => Key::Unicode('r'),
        K::S => Key::Unicode('s'),
        K::T => Key::Unicode('t'),
        K::U => Key::Unicode('u'),
        K::V => Key::Unicode('v'),
        K::W => Key::Unicode('w'),
        K::X => Key::Unicode('x'),
        K::Y => Key::Unicode('y'),
        K::Z => Key::Unicode('z'),
        K::D0 => Key::Unicode('0'),
        K::D1 => Key::Unicode('1'),
        K::D2 => Key::Unicode('2'),
        K::D3 => Key::Unicode('3'),
        K::D4 => Key::Unicode('4'),
        K::D5 => Key::Unicode('5'),
        K::D6 => Key::Unicode('6'),
        K::D7 => Key::Unicode('7'),
        K::D8 => Key::Unicode('8'),
        K::D9 => Key::Unicode('9'),
        K::F1 => Key::F1,
        K::F2 => Key::F2,
        K::F3 => Key::F3,
        K::F4 => Key::F4,
        K::F5 => Key::F5,
        K::F6 => Key::F6,
        K::F7 => Key::F7,
        K::F8 => Key::F8,
        K::F9 => Key::F9,
        K::F10 => Key::F10,
        K::F11 => Key::F11,
        K::F12 => Key::F12,
        K::ArrowUp => Key::UpArrow,
        K::ArrowDown => Key::DownArrow,
        K::ArrowLeft => Key::LeftArrow,
        K::ArrowRight => Key::RightArrow,
        K::Enter => Key::Return,
        K::Escape => Key::Escape,
        K::Tab => Key::Tab,
        K::Space => Key::Space,
        K::Backspace => Key::Backspace,
        K::Delete => Key::Delete,
        K::LShift | K::RShift => Key::Shift,
        K::LCtrl | K::RCtrl => Key::Control,
        K::LAlt | K::RAlt => Key::Alt,
        K::LCmd | K::RCmd => Key::Meta,
        // Kernel KeyCode is #[non_exhaustive]: a variant added upstream that
        // this crate does not know yet MUST fail closed (never guess a key).
        _ => {
            return Err(InputError::EventPostFailed {
                reason: "unmapped KeyCode variant (kernel newer than nika-input)".to_owned(),
            });
        }
    })
}

/// Expand the kernel's 4-modifier set into enigo keys to hold, in the
/// canonical field order `cmd · shift · alt · ctrl`.
pub(crate) fn mods_keys(mods: KeyMods) -> Vec<Key> {
    let mut held = Vec::with_capacity(4);
    if mods.cmd {
        held.push(Key::Meta);
    }
    if mods.shift {
        held.push(Key::Shift);
    }
    if mods.alt {
        held.push(Key::Alt);
    }
    if mods.ctrl {
        held.push(Key::Control);
    }
    held
}

/// A fully-resolved key chord: the enigo modifiers to hold around the tap.
/// PURE output of [`chord_plan`] — all mapping/fail-closed decisions happen
/// here, headless-testable; the executor below is pure FFI residue.
pub(crate) struct ChordPlan {
    pub(crate) held: Vec<Key>,
    pub(crate) tap: Key,
}

/// Resolve a kernel `(key, mods)` chord into the enigo sequence — **pure**.
///
/// # Errors
/// Fail-closed mapping errors from [`map_key`] (unknown kernel variant).
pub(crate) fn chord_plan(key: KeyCode, mods: KeyMods) -> Result<ChordPlan, InputError> {
    Ok(ChordPlan {
        held: mods_keys(mods),
        tap: map_key(key)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_key_exhaustive_over_all_66_kernel_variants() {
        // EVERY kernel variant pinned to its exact enigo key — kills every
        // per-arm deletion mutant in the map (Gate 5).
        use KeyCode as K;
        let unicode: [(KeyCode, char); 36] = [
            (K::A, 'a'),
            (K::B, 'b'),
            (K::C, 'c'),
            (K::D, 'd'),
            (K::E, 'e'),
            (K::F, 'f'),
            (K::G, 'g'),
            (K::H, 'h'),
            (K::I, 'i'),
            (K::J, 'j'),
            (K::K, 'k'),
            (K::L, 'l'),
            (K::M, 'm'),
            (K::N, 'n'),
            (K::O, 'o'),
            (K::P, 'p'),
            (K::Q, 'q'),
            (K::R, 'r'),
            (K::S, 's'),
            (K::T, 't'),
            (K::U, 'u'),
            (K::V, 'v'),
            (K::W, 'w'),
            (K::X, 'x'),
            (K::Y, 'y'),
            (K::Z, 'z'),
            (K::D0, '0'),
            (K::D1, '1'),
            (K::D2, '2'),
            (K::D3, '3'),
            (K::D4, '4'),
            (K::D5, '5'),
            (K::D6, '6'),
            (K::D7, '7'),
            (K::D8, '8'),
            (K::D9, '9'),
        ];
        for (k, c) in unicode {
            assert_eq!(map_key(k).expect("maps"), Key::Unicode(c), "{k:?}");
        }
        let named: [(KeyCode, Key); 30] = [
            (K::F1, Key::F1),
            (K::F2, Key::F2),
            (K::F3, Key::F3),
            (K::F4, Key::F4),
            (K::F5, Key::F5),
            (K::F6, Key::F6),
            (K::F7, Key::F7),
            (K::F8, Key::F8),
            (K::F9, Key::F9),
            (K::F10, Key::F10),
            (K::F11, Key::F11),
            (K::F12, Key::F12),
            (K::ArrowUp, Key::UpArrow),
            (K::ArrowDown, Key::DownArrow),
            (K::ArrowLeft, Key::LeftArrow),
            (K::ArrowRight, Key::RightArrow),
            (K::Enter, Key::Return),
            (K::Escape, Key::Escape),
            (K::Tab, Key::Tab),
            (K::Space, Key::Space),
            (K::Backspace, Key::Backspace),
            (K::Delete, Key::Delete),
            // sided modifiers COLLAPSE to the ungated cross-platform keys
            (K::LShift, Key::Shift),
            (K::RShift, Key::Shift),
            (K::LCtrl, Key::Control),
            (K::RCtrl, Key::Control),
            (K::LAlt, Key::Alt),
            (K::RAlt, Key::Alt),
            (K::LCmd, Key::Meta),
            (K::RCmd, Key::Meta),
        ];
        for (k, want) in named {
            assert_eq!(map_key(k).expect("maps"), want, "{k:?}");
        }
    }

    #[test]
    fn mods_keys_canonical_order_cmd_shift_alt_ctrl() {
        assert!(mods_keys(KeyMods::default()).is_empty());
        let all = mods_keys(KeyMods::new(true, true, true, true));
        assert!(matches!(
            all.as_slice(),
            [Key::Meta, Key::Shift, Key::Alt, Key::Control]
        ));
        let cmd_ctrl = mods_keys(KeyMods::new(true, false, false, true));
        assert!(matches!(cmd_ctrl.as_slice(), [Key::Meta, Key::Control]));
    }

    #[test]
    fn chord_plan_resolves_held_and_tap() {
        let plan = chord_plan(KeyCode::C, KeyMods::new(true, false, false, false))
            .expect("cmd+c resolves");
        assert!(matches!(plan.held.as_slice(), [Key::Meta]));
        assert_eq!(plan.tap, Key::Unicode('c'));
        // Unknown-variant fail-closed propagates through the plan (cannot be
        // exercised today — all 66 variants map — pinned via map_key directly).
        let bare = chord_plan(KeyCode::Enter, KeyMods::default()).expect("bare key");
        assert!(bare.held.is_empty());
        assert_eq!(bare.tap, Key::Return);
    }
}
