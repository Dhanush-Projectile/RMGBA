/* preferences.rs
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

//! Preferences window: a visual GBA gamepad plus an interactive keyboard
//! remapping list. Each control has a "capture" button; clicking it records the
//! next key press and persists it through `Controls` (GSettings).

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk::glib;
use gtk::glib::translate::IntoGlib;
use gtk::{gdk, EventControllerKey};

use crate::settings::{Control, Controls};

/// Keyvals that are only modifiers (never useful as a control binding).
fn is_modifier(key: gdk::Key) -> bool {
    matches!(
        key,
        gdk::Key::Shift_L
            | gdk::Key::Shift_R
            | gdk::Key::Control_L
            | gdk::Key::Control_R
            | gdk::Key::Alt_L
            | gdk::Key::Alt_R
            | gdk::Key::Meta_L
            | gdk::Key::Meta_R
            | gdk::Key::Super_L
            | gdk::Key::Super_R
            | gdk::Key::Caps_Lock
            | gdk::Key::Num_Lock
            | gdk::Key::Scroll_Lock
    )
}

/// Human readable name for a key (falls back to the numeric keyval).
fn key_label(key: gdk::Key) -> String {
    key.name()
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("{}", key.into_glib()))
}

struct ControlsUi {
    controls: Controls,
    buttons: RefCell<std::collections::HashMap<Control, gtk::Button>>,
    recording: RefCell<Option<Control>>,
}

impl ControlsUi {
    fn new(controls: Controls) -> Rc<Self> {
        Rc::new(Self {
            controls,
            buttons: RefCell::new(std::collections::HashMap::new()),
            recording: RefCell::new(None),
        })
    }

    /// (Re)syncs every capture button label to the currently configured key.
    fn refresh(&self) {
        for (control, button) in self.buttons.borrow().iter() {
            button.set_label(&key_label(self.controls.key_for(*control)));
        }
    }

    /// Puts `control` into (or out of) recording mode. Only one control may be
    /// recording at a time.
    fn toggle_recording(&self, control: Control) {
        let mut active = self.recording.borrow_mut();
        if *active == Some(control) {
            *active = None;
            self.refresh();
            return;
        }
        *active = Some(control);
        if let Some(button) = self.buttons.borrow().get(&control) {
            button.set_label("Press a key…");
        }
    }

    /// Applies a freshly captured keyval to the control being recorded.
    fn capture_key(&self, key: gdk::Key) -> bool {
        let Some(control) = *self.recording.borrow() else {
            return false;
        };
        if key == gdk::Key::Escape {
            *self.recording.borrow_mut() = None;
            self.refresh();
            return true;
        }
        if key.into_glib() == 0 || is_modifier(key) {
            return true;
        }
        self.controls.set_key(control, key);
        *self.recording.borrow_mut() = None;
        self.refresh();
        true
    }
}

/// Builds and presents the preferences dialog for `application`.
pub fn show_preferences(application: &adw::Application) {
    let dialog = adw::PreferencesDialog::new();

    let ui = ControlsUi::new(Controls::new());

    // General page (kept empty for now).
    let general_page = adw::PreferencesPage::new();
    general_page.set_title("General");
    dialog.add(&general_page);

    // Controls page.
    let page = adw::PreferencesPage::new();
    page.set_title("Controls");
    page.set_description("Adjust the keyboard controls for the GBA.");

    // Per-control key binding rows.
    let mapping_group = adw::PreferencesGroup::new();
    mapping_group.set_title("Key Bindings");
    mapping_group.set_description(Some(
        "Click a button, then press the key you want to use.",
    ));

    for control in [
        Control::A,
        Control::B,
        Control::L,
        Control::R,
        Control::Select,
        Control::Start,
        Control::Up,
        Control::Down,
        Control::Left,
        Control::Right,
    ] {
        let capture = gtk::Button::new();
        capture.add_css_class("flat");
        capture.set_label(&key_label(ui.controls.key_for(control)));

        let row = adw::ActionRow::new();
        row.set_title(control.label());
        row.set_subtitle("Key binding");
        row.set_activatable_widget(Some(&capture));
        row.add_suffix(&capture);

        ui.buttons.borrow_mut().insert(control, capture.clone());

        let ui_clone = Rc::clone(&ui);
        capture.connect_clicked(move |_| ui_clone.toggle_recording(control));

        mapping_group.add(&row);
    }
    page.add(&mapping_group);

    dialog.add(&page);

    // Dialog-level key capture: when a control is recording, the next keypress
    // binds it and is consumed so it doesn't also trigger the app.
    let key_controller = EventControllerKey::new();
    let ui = Rc::clone(&ui);
    key_controller.connect_key_pressed(
        move |_, key, _, _| {
            let active = ui.recording.borrow().is_some();
            if active && ui.capture_key(key) {
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        },
    );
    dialog.add_controller(key_controller);

    dialog.present(application.active_window().as_ref());
}
