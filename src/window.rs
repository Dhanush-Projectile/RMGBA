/* window.rs
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

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::prelude::*;
use gtk::CompositeTemplate;
use gtk::{gdk, gio, glib, EventControllerKey};

use crate::emulator::{self, EmuEvent, Emulator};
use mgba::{GBA_HEIGHT, GBA_WIDTH};

mod imp {
    use super::*;

    #[derive(Default, CompositeTemplate)]
    #[template(resource = "/app/rmgba/game/window.ui")]
    pub struct RmgbaWindow {
        // Template widgets
        #[template_child]
        pub picture: TemplateChild<gtk::Picture>,
        #[template_child]
        pub toast_overlay: TemplateChild<adw::ToastOverlay>,

        pub emulator: RefCell<Option<Emulator>>,
        pub keys: Arc<AtomicU32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for RmgbaWindow {
        const NAME: &'static str = "RmgbaWindow";
        type Type = super::RmgbaWindow;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
            klass.install_action("win.open", None, |win, _, _| win.open_rom());
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for RmgbaWindow {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.setup_key_input();
            obj.connect_close_request(|_| glib::Propagation::Proceed);
            obj.connect_destroy(|win| {
                win.imp().emulator.take();
            });
        }
    }
    impl WidgetImpl for RmgbaWindow {}
    impl WindowImpl for RmgbaWindow {}
    impl ApplicationWindowImpl for RmgbaWindow {}
    impl AdwApplicationWindowImpl for RmgbaWindow {}
}

glib::wrapper! {
    pub struct RmgbaWindow(ObjectSubclass<imp::RmgbaWindow>)
        @extends gtk::Widget, gtk::Window, gtk::ApplicationWindow, adw::ApplicationWindow,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl RmgbaWindow {
    pub fn new<P: IsA<gtk::Application>>(application: &P) -> Self {
        glib::Object::builder()
            .property("application", application)
            .build()
    }

    fn open_rom(&self) {
        let filter = gtk::FileFilter::new();
        filter.set_name(Some(&gettext("GBA ROMs")));
        filter.add_pattern("*.gba");

        let filters = gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&filter);

        let dialog = gtk::FileDialog::builder()
            .title(gettext("Open ROM"))
            .filters(&filters)
            .modal(true)
            .build();

        dialog.open(
            Some(self),
            None::<&gio::Cancellable>,
            glib::clone!(
                #[weak(rename_to = window)]
                self,
                move |result| {
                    if let Ok(file) = result {
                        if let Some(path) = file.path() {
                            window.start_emulation(path);
                        }
                    }
                }
            ),
        );
    }

    fn start_emulation(&self, path: PathBuf) {
        // Stop any running instance first.
        self.imp().emulator.replace(None);

        // Marshal events from the emulation thread to this widget.
        // SendWeakRef is the Send+Sync weak reference for GTK objects.
        let window_weak: glib::SendWeakRef<RmgbaWindow> = self.downgrade().into();
        let on_event = Arc::new(move |event: EmuEvent| {
            let Some(window) = window_weak.upgrade() else {
                return;
            };
            match event {
                EmuEvent::Frame(pixels) => window.show_frame(&pixels),
                EmuEvent::Error(message) => window.show_error(&message),
            }
        });

        match Emulator::start(path, Arc::clone(&self.imp().keys), on_event) {
            Ok(emulator) => {
                self.imp().emulator.replace(Some(emulator));
            }
            Err(message) => self.show_error(&message),
        }
    }

    /// Uploads one frame to a texture and displays it.
    ///
    /// The core's display buffer holds 0xAARRGGBB words; read little-endian
    /// that is exactly GDK's Bgra8 memory format.
    fn show_frame(&self, pixels: &[u32]) {
        if pixels.len() != GBA_WIDTH * GBA_HEIGHT {
            return;
        }
        let bytes =
            unsafe { std::slice::from_raw_parts(pixels.as_ptr() as *const u8, pixels.len() * 4) };
        let texture = gdk::MemoryTexture::new(
            GBA_WIDTH as i32,
            GBA_HEIGHT as i32,
            gdk::MemoryFormat::B8g8r8a8Premultiplied,
            &glib::Bytes::from(bytes),
            GBA_WIDTH * 4,
        );
        self.imp().picture.set_paintable(Some(&texture));
    }

    fn show_error(&self, message: &str) {
        eprintln!("{message}");
        self.imp()
            .toast_overlay
            .add_toast(adw::Toast::new(message));
    }

    fn setup_key_input(&self) {
        let controller = EventControllerKey::new();
        controller.connect_key_pressed(
            glib::clone!(
                #[weak(rename_to = window)]
                self,
                #[upgrade_or]
                glib::Propagation::Proceed,
                move |_, keyval, _, _| {
                    if let Some(key) = map_key(keyval) {
                        window.imp().keys.fetch_or(key, Ordering::Relaxed);
                        return glib::Propagation::Stop;
                    }
                    glib::Propagation::Proceed
                }
            ),
        );
        controller.connect_key_released(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_, keyval, _, _| {
                if let Some(key) = map_key(keyval) {
                    window.imp().keys.fetch_and(!key, Ordering::Relaxed);
                }
            }
        ));
        self.add_controller(controller);
    }
}

fn map_key(keyval: gdk::Key) -> Option<u32> {
    use emulator::*;
    match keyval {
        gdk::Key::x => Some(KEY_A),
        gdk::Key::z => Some(KEY_B),
        gdk::Key::BackSpace => Some(KEY_SELECT),
        gdk::Key::Return | gdk::Key::KP_Enter => Some(KEY_START),
        gdk::Key::Right => Some(KEY_RIGHT),
        gdk::Key::Left => Some(KEY_LEFT),
        gdk::Key::Up => Some(KEY_UP),
        gdk::Key::Down => Some(KEY_DOWN),
        gdk::Key::a => Some(KEY_L),
        gdk::Key::s => Some(KEY_R),
        _ => None,
    }
}
