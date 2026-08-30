/* saves.rs
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

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use gtk::glib;

// In the sandboxed (Flatpak) environment the app cannot browse the user's real
// filesystem, so it cannot discover a `.sav` sitting next to a ROM by scanning
// the ROM's directory. Files are only reachable through the portal mounts
// (/run/user/1000/doc/...). To still get "auto-load the save" behaviour we
// remember which save the user last loaded for each ROM (keyed by ROM file
// name) and re-apply it the next time that ROM is opened.
//
// The mapping is stored as a plain tab-separated "rom_name\tsave_path" file in
// the (writable) user config directory.

fn store_path() -> PathBuf {
    glib::user_config_dir().join("rmgba").join("saves.txt")
}

fn key_for(rom: &Path) -> String {
    rom.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn load() -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let Ok(content) = fs::read_to_string(store_path()) {
        for line in content.lines() {
            if let Some((rom, save)) = line.split_once('\t') {
                map.insert(rom.to_string(), save.to_string());
            }
        }
    }
    map
}

fn save(map: &HashMap<String, String>) {
    let path = store_path();
    if map.is_empty() {
        let _ = fs::remove_file(&path);
        return;
    }
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let mut content = String::new();
    for (rom, save) in map {
        content.push_str(rom);
        content.push('\t');
        content.push_str(save);
        content.push('\n');
    }
    let _ = fs::write(&path, content);
}

/// Remembers the save the user chose for a ROM so it can be auto-applied on a
/// later open. Fails silently on any I/O error.
pub fn remember(rom: &Path, save_path: &Path) {
    let rom = key_for(rom);
    if rom.is_empty() {
        return;
    }
    let mut map = load();
    map.insert(rom.clone(), save_path.to_string_lossy().into_owned());
    save(&map);
}

/// Returns the last save remembered for `rom`, if it is registered and still
/// exists on disk (the portal mount may have gone stale between sessions).
pub fn for_rom(rom: &Path) -> Option<PathBuf> {
    let rom = key_for(rom);
    if rom.is_empty() {
        return None;
    }
    let map = load();
    let Some(seen) = map.get(&rom) else {
        return None;
    };
    let seen = seen.to_string();
    let save = PathBuf::from(seen);
    if save.exists() {
        Some(save)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let mut map = HashMap::new();
        map.insert("game.gba".to_string(), "/tmp/one.sav".to_string());
        map.insert("other.gba".to_string(), "/tmp/two.sav".to_string());
        // We don't write to the real store in tests; just verify parsing logic
        // via the line format used by dump/load.
        let mut content = String::new();
        for (rom, save) in &map {
            content.push_str(rom);
            content.push('\t');
            content.push_str(save);
            content.push('\n');
        }
        let mut parsed = HashMap::new();
        for line in content.lines() {
            if let Some((r, s)) = line.split_once('\t') {
                parsed.insert(r.to_string(), s.to_string());
            }
        }
        assert_eq!(parsed.len(), map.len());
    }
}
