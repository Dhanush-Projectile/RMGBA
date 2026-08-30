/* settings.rs
 *
 * Copyright 2026 Dhanush
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

//! Keyboard mapping for the GBA controller, persisted via GSettings.
//!
//! Every GBA control (A, B, Start, Select, the D-Pad and the L/R shoulders)
//! is mapped to a GDK key. The mapping is stored in the app's GSettings schema
//! (`app.rmgba.game`, keys `key-*`). A stored value of 0 means "use the
//! built-in default", which matches the classic RMGBA layout.

use gtk::gdk;
use gtk::gio;
use gtk::glib::translate::{FromGlib, IntoGlib};
use gio::prelude::*;

/// A single GBA controller button.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Control {
    A,
    B,
    Select,
    Start,
    Up,
    Down,
    Left,
    Right,
    L,
    R,
}

impl Control {
    /// GSettings key under which this control's keyval is stored.
    pub fn setting_key(self) -> &'static str {
        match self {
            Control::A => "key-a",
            Control::B => "key-b",
            Control::Select => "key-select",
            Control::Start => "key-start",
            Control::Up => "key-up",
            Control::Down => "key-down",
            Control::Left => "key-left",
            Control::Right => "key-right",
            Control::L => "key-l",
            Control::R => "key-r",
        }
    }

    /// The key bit this control drives on the emulator core (see emulator::KEY_*).
    pub fn bit(self) -> u32 {
        match self {
            Control::A => crate::emulator::KEY_A,
            Control::B => crate::emulator::KEY_B,
            Control::Select => crate::emulator::KEY_SELECT,
            Control::Start => crate::emulator::KEY_START,
            Control::Up => crate::emulator::KEY_UP,
            Control::Down => crate::emulator::KEY_DOWN,
            Control::Left => crate::emulator::KEY_LEFT,
            Control::Right => crate::emulator::KEY_RIGHT,
            Control::L => crate::emulator::KEY_L,
            Control::R => crate::emulator::KEY_R,
        }
    }

    /// Display name shown in the preferences UI.
    pub fn label(self) -> &'static str {
        match self {
            Control::A => "A",
            Control::B => "B",
            Control::Select => "Select",
            Control::Start => "Start",
            Control::Up => "D-Pad Up",
            Control::Down => "D-Pad Down",
            Control::Left => "D-Pad Left",
            Control::Right => "D-Pad Right",
            Control::L => "L (Shoulder)",
            Control::R => "R (Shoulder)",
        }
    }

    /// The key assigned when a control has no explicit (non-zero) setting.
    pub fn default_key(self) -> gdk::Key {
        match self {
            Control::A => gdk::Key::x,
            Control::B => gdk::Key::z,
            Control::Select => gdk::Key::BackSpace,
            Control::Start => gdk::Key::Return,
            Control::Up => gdk::Key::Up,
            Control::Down => gdk::Key::Down,
            Control::Left => gdk::Key::Left,
            Control::Right => gdk::Key::Right,
            Control::L => gdk::Key::a,
            Control::R => gdk::Key::s,
        }
    }
}

/// The GSettings-backed controller configuration.
#[derive(Clone)]
pub struct Controls {
    settings: gio::Settings,
}

impl Controls {
    pub fn new() -> Self {
        Self {
            settings: gio::Settings::new("app.rmgba.game"),
        }
    }

    /// The keyboard key currently bound to `control`.
    pub fn key_for(&self, control: Control) -> gdk::Key {
        let val = self.settings.uint(control.setting_key());
        if val == 0 {
            control.default_key()
        } else {
            // SAFETY: any u32 round-trips to a gdk::Key; invalid keyvals are
            // simply never matched by real key events.
            unsafe { gdk::Key::from_glib(val) }
        }
    }

    /// Binds `key` to `control`. A `0` keyval would clear the setting back to
    /// the built-in default; normal keys are always non-zero.
    pub fn set_key(&self, control: Control, key: gdk::Key) {
        let keyval = key.into_glib();
        let _ = self.settings.set_uint(control.setting_key(), keyval);
    }

    /// Returns the emulator key *bit* bound to `keyval`, if any. Reads the
    /// current GSettings values directly so remapping applies with no signal
    /// bookkeeping.
    pub fn bit_for_keyval(&self, keyval: u32) -> Option<u32> {
        for control in [
            Control::A,
            Control::B,
            Control::Select,
            Control::Start,
            Control::Up,
            Control::Down,
            Control::Left,
            Control::Right,
            Control::L,
            Control::R,
        ] {
            if self.key_for(control).into_glib() == keyval {
                return Some(control.bit());
            }
        }
        None
    }
}

impl Default for Controls {
    fn default() -> Self {
        Self::new()
    }
}
