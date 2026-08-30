/* application.rs
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

use gettextrs::gettext;
use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gdk, gio, glib};

use crate::config::VERSION;
use crate::preferences;
use crate::RmgbaWindow;

mod imp {
    use super::*;

    #[derive(Debug, Default)]
    pub struct RmgbaApplication {}

    #[glib::object_subclass]
    impl ObjectSubclass for RmgbaApplication {
        const NAME: &'static str = "RmgbaApplication";
        type Type = super::RmgbaApplication;
        type ParentType = adw::Application;
    }

    impl ObjectImpl for RmgbaApplication {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.setup_gactions();
            obj.set_accels_for_action("app.quit", &["<control>q"]);
            obj.set_accels_for_action("win.open", &["<control>o"]);
            obj.set_accels_for_action("win.load_save", &["<control>l"]);
        }
    }

    impl ApplicationImpl for RmgbaApplication {
        fn startup(&self) {
            // Runs after GTK has been initialized by the application.
            self.parent_startup();
            self.obj().setup_css();
        }

        // We connect to the activate callback to create a window when the application
        // has been launched. Additionally, this callback notifies us when the user
        // tries to launch a "second instance" of the application. When they try
        // to do that, we'll just present any existing window.
        fn activate(&self) {
            let application = self.obj();
            // Get the current window or create one if necessary
            let window = application.active_window().unwrap_or_else(|| {
                let window = RmgbaWindow::new(&*application);
                window.upcast()
            });

            // Ask the window manager/compositor to present the window
            window.present();
        }
    }

    impl GtkApplicationImpl for RmgbaApplication {}
    impl AdwApplicationImpl for RmgbaApplication {}
}

glib::wrapper! {
    pub struct RmgbaApplication(ObjectSubclass<imp::RmgbaApplication>)
        @extends gio::Application, gtk::Application, adw::Application,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl RmgbaApplication {
    pub fn new(application_id: &str, flags: &gio::ApplicationFlags) -> Self {
        glib::Object::builder()
            .property("application-id", application_id)
            .property("flags", flags)
            .property("resource-base-path", "/app/rmgba/game")
            .build()
    }

    fn setup_gactions(&self) {
        let quit_action = gio::ActionEntry::builder("quit")
            .activate(move |app: &Self, _, _| app.quit())
            .build();
        let about_action = gio::ActionEntry::builder("about")
            .activate(move |app: &Self, _, _| app.show_about())
            .build();
        let preferences_action = gio::ActionEntry::builder("preferences")
            .activate(move |app: &Self, _, _| {
                let app: &adw::Application = app.upcast_ref();
                preferences::show_preferences(app);
            })
            .build();
        self.add_action_entries([quit_action, about_action, preferences_action]);
    }

    fn setup_css(&self) {
        // Solid black backdrop for the video area so letterboxing around the
        // 3:2 frame is distinguishable from the window background.
        let provider = gtk::CssProvider::new();
        provider.load_from_string(
            ".game-view { background-color: black; }",
        );
        gtk::style_context_add_provider_for_display(
            &gdk::Display::default().expect("could not connect to display"),
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    fn show_about(&self) {
        let window = self.active_window().unwrap();
        let about = adw::AboutDialog::builder()
            .application_name("RMGBA")
            .application_icon("app.rmgba.game")
            .developer_name("Dhanush")
            .version(VERSION)
            .developers(vec!["Dhanush"])
            // Translators: Replace "translator-credits" with your name/username, and optionally an email or URL.
            .translator_credits(gettext("translator-credits"))
            .copyright("© 2026 Dhanush")
            .build();

        about.present(Some(&window));
    }
}
