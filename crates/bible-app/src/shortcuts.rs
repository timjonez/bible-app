//! Keyboard Shortcuts dialog (Ctrl+? or the menu).
//!
//! One table drives the dialog. Keep it in step with the key handlers in
//! `app.rs` and the Shortcuts table in the README.
use adw::prelude::*;
use relm4::{adw, gtk};

pub struct Shortcut {
    /// Keys pressed together, in order, as shown on their keycaps.
    pub keys: &'static [&'static str],
    pub what: &'static str,
}

pub struct Section {
    pub title: &'static str,
    pub shortcuts: &'static [Shortcut],
}

pub const SECTIONS: &[Section] = &[
    Section {
        title: "Getting around",
        shortcuts: &[
            Shortcut {
                keys: &["Ctrl", "L"],
                what: "Go to a reference",
            },
            Shortcut {
                keys: &["Shift", "Enter"],
                what: "Open the reference beside (in Go to)",
            },
            Shortcut {
                keys: &["Alt", "←"],
                what: "Previous chapter",
            },
            Shortcut {
                keys: &["Alt", "→"],
                what: "Next chapter",
            },
            Shortcut {
                keys: &["Alt", "Shift", "←"],
                what: "Back in history",
            },
            Shortcut {
                keys: &["Alt", "Shift", "→"],
                what: "Forward in history",
            },
            Shortcut {
                keys: &["Ctrl", "T"],
                what: "New tab",
            },
        ],
    },
    Section {
        title: "Search",
        shortcuts: &[
            Shortcut {
                keys: &["Ctrl", "F"],
                what: "Search",
            },
            Shortcut {
                keys: &["↑", "↓"],
                what: "Preview a hit",
            },
            Shortcut {
                keys: &["Enter"],
                what: "Open the hit beside the list",
            },
            Shortcut {
                keys: &["Esc"],
                what: "Close Search and return to your passage",
            },
        ],
    },
    Section {
        title: "Reading and marks",
        shortcuts: &[
            Shortcut {
                keys: &["Ctrl", "+"],
                what: "Larger text",
            },
            Shortcut {
                keys: &["Ctrl", "−"],
                what: "Smaller text",
            },
            Shortcut {
                keys: &["Ctrl", "C"],
                what: "Copy the selection or verse with its citation",
            },
            Shortcut {
                keys: &["Ctrl", "D"],
                what: "Bookmark the verse",
            },
            Shortcut {
                keys: &["Ctrl", "Shift", "N"],
                what: "Add or edit a note",
            },
        ],
    },
    Section {
        title: "Mouse",
        shortcuts: &[
            Shortcut {
                keys: &["Click a word"],
                what: "Strong’s, lexicons, and dictionaries",
            },
            Shortcut {
                keys: &["Click a letter"],
                what: "Treasury cross-references for that phrase",
            },
            Shortcut {
                keys: &["Right-click a verse"],
                what: "Copy, bookmark, highlight, note, commentary",
            },
        ],
    },
    Section {
        title: "Help",
        shortcuts: &[Shortcut {
            keys: &["Ctrl", "?"],
            what: "Keyboard shortcuts",
        }],
    },
];

/// Spoken form of a shortcut, for screen readers.
pub fn spoken(keys: &[&str]) -> String {
    keys.join(" plus ")
}

/// Ctrl+? (Ctrl+Shift+/ on most layouts).
pub fn is_shortcuts_key(keyval: gtk::gdk::Key, ctrl: bool) -> bool {
    ctrl && (keyval == gtk::gdk::Key::question || keyval == gtk::gdk::Key::slash)
}

/// Open the dialog over whichever window has focus.
pub fn present_for_active_window() {
    let window = relm4::main_application().active_window();
    present(window.as_ref());
}

pub fn present(parent: Option<&impl IsA<gtk::Widget>>) {
    install_css();
    let page = adw::PreferencesPage::new();
    for section in SECTIONS {
        let group = adw::PreferencesGroup::new();
        group.set_title(section.title);
        for shortcut in section.shortcuts {
            let row = adw::ActionRow::new();
            row.set_title(shortcut.what);
            row.add_suffix(&keycaps(shortcut.keys));
            row.upcast_ref::<gtk::Widget>().update_property(&[
                gtk::accessible::Property::Description(&spoken(shortcut.keys)),
            ]);
            group.add(&row);
        }
        page.add(&group);
    }
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());
    toolbar.set_content(Some(&page));
    let dialog = adw::Dialog::new();
    dialog.set_title("Keyboard Shortcuts");
    dialog.set_content_width(520);
    dialog.set_content_height(640);
    dialog.set_child(Some(&toolbar));
    dialog.present(parent);
}

fn keycaps(keys: &[&str]) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    row.set_valign(gtk::Align::Center);
    for key in keys {
        let cap = gtk::Label::new(Some(key));
        cap.add_css_class("shortcut-key");
        row.append(&cap);
    }
    row
}

fn install_css() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let provider = gtk::CssProvider::new();
        provider.load_from_string(
            r#"
            label.shortcut-key {
              padding: 2px 8px;
              border-radius: 6px;
              background-color: alpha(currentColor, 0.08);
              box-shadow: inset 0 -1px alpha(currentColor, 0.18);
              font-size: 0.9em;
              font-weight: 600;
            }
            "#,
        );
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all() -> impl Iterator<Item = &'static Shortcut> {
        SECTIONS.iter().flat_map(|s| s.shortcuts.iter())
    }

    #[test]
    fn every_shortcut_has_keys_and_a_description() {
        for shortcut in all() {
            assert!(!shortcut.keys.is_empty(), "{}", shortcut.what);
            assert!(
                shortcut.keys.iter().all(|k| !k.is_empty()),
                "{}",
                shortcut.what
            );
            assert!(!shortcut.what.is_empty());
        }
    }

    #[test]
    fn lists_the_keys_the_window_handles() {
        let has = |keys: &[&str]| all().any(|s| s.keys == keys);
        for keys in [
            &["Ctrl", "L"][..],
            &["Ctrl", "F"],
            &["Ctrl", "T"],
            &["Ctrl", "D"],
            &["Ctrl", "Shift", "N"],
            &["Alt", "←"],
            &["Alt", "Shift", "→"],
            &["Ctrl", "?"],
        ] {
            assert!(has(keys), "missing {keys:?}");
        }
    }

    #[test]
    fn readme_lists_the_same_keyboard_shortcuts() {
        let readme = include_str!("../../../README.md");
        for needle in [
            "Ctrl+L",
            "Ctrl+F",
            "Ctrl+T",
            "Ctrl+D",
            "Ctrl+Shift+N",
            "Ctrl+?",
        ] {
            assert!(readme.contains(needle), "README is missing {needle}");
        }
    }

    #[test]
    fn spoken_joins_keys() {
        assert_eq!(spoken(&["Ctrl", "Shift", "N"]), "Ctrl plus Shift plus N");
    }
}
