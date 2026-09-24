//! Window and reading colors.
//!
//! On Omarchy the palette is `colors.toml` from the current theme
//! (`$XDG_STATE_HOME/omarchy/current/theme/colors.toml`). `omarchy theme set`
//! replaces that directory; the app reloads when it changes.
//! `BIBLE_APP_THEME` can point at another `colors.toml`.
//! Without either file, chrome stays with libadwaita and the reading colors
//! follow its dark/light scheme and accent.

use adw::prelude::*;
use relm4::{adw, gtk};
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Mutex;
use std::time::Duration;

const MIN_INK: f32 = 3.0;
const MIN_FILL: f32 = 4.5;

#[derive(Clone, Debug, PartialEq)]
pub struct Palette {
    pub dark: bool,
    /// Recolor window chrome from this palette. False follows libadwaita.
    pub chrome: bool,
    pub background: String,
    pub foreground: String,
    pub accent: String,
    pub accent_fg: String,
    pub muted: String,
    pub dim: String,
    pub header: String,
    pub elevated: String,
    pub yellow: String,
    pub green: String,
    pub blue: String,
    pub red: String,
    pub orange: String,
    pub magenta: String,
    pub destructive_fg: String,
    pub success_fg: String,
    pub warning_fg: String,
}

pub struct Watch {
    _provider: gtk::CssProvider,
    _monitors: Vec<gtk::gio::FileMonitor>,
    _reload: Rc<Reload>,
}

static CURRENT: Mutex<Option<Palette>> = Mutex::new(None);

pub fn current() -> Palette {
    CURRENT
        .lock()
        .expect("theme lock")
        .clone()
        .unwrap_or_else(adwaita_dark)
}

struct Reload {
    gen: Cell<u64>,
    slot: RefCell<Option<gtk::gio::FileMonitor>>,
    provider: gtk::CssProvider,
    on_change: Box<dyn Fn()>,
}

impl Reload {
    fn kick(self: &Rc<Self>) {
        let n = self.gen.get().wrapping_add(1);
        self.gen.set(n);
        let this = Rc::clone(self);
        gtk::glib::timeout_add_local(Duration::from_millis(200), move || {
            if this.gen.get() != n {
                return gtk::glib::ControlFlow::Break;
            }
            match publish(&this.provider, true) {
                Publish::Changed => (this.on_change)(),
                Publish::Same => {}
                Publish::Wait => {
                    let retry = Rc::clone(&this);
                    gtk::glib::timeout_add_local(Duration::from_millis(300), move || {
                        if retry.gen.get() != n {
                            return gtk::glib::ControlFlow::Break;
                        }
                        if publish(&retry.provider, false) == Publish::Changed {
                            (retry.on_change)();
                        }
                        gtk::glib::ControlFlow::Break
                    });
                }
            }
            let rearm = Rc::clone(&this);
            gtk::glib::idle_add_local(move || {
                arm_color_file(&rearm);
                gtk::glib::ControlFlow::Break
            });
            gtk::glib::ControlFlow::Break
        });
    }
}

pub fn install(on_change: impl Fn() + 'static) -> Watch {
    let provider = gtk::CssProvider::new();
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_USER + 50,
        );
    }

    let reload = Rc::new(Reload {
        gen: Cell::new(0),
        slot: RefCell::new(None),
        provider: provider.clone(),
        on_change: Box::new(on_change),
    });
    arm_color_file(&reload);
    let monitors = arm_directories(&reload);
    let style = adw::StyleManager::default();
    let on_dark = Rc::clone(&reload);
    style.connect_dark_notify(move |_| on_dark.kick());
    let on_accent = Rc::clone(&reload);
    style.connect_accent_color_notify(move |_| on_accent.kick());

    publish(&provider, false);

    Watch {
        _provider: provider,
        _monitors: monitors,
        _reload: reload,
    }
}

pub fn paint_buffer(buffer: &gtk::TextBuffer) {
    let palette = current();
    set_fg(buffer, "verse-num", &palette.muted);
    set_fg(buffer, "note", &palette.dim);
    set_fg(buffer, "note-mark", &palette.dim);
    set_fg(buffer, "apparatus", &palette.muted);
    set_fg(buffer, "xref", &palette.accent);
    set_fg(buffer, "mhc-num", &palette.accent);
    set_fg(buffer, "tsk-sup", &palette.accent);
    set_fg(buffer, "lemma", &palette.dim);
    set_fg(buffer, "current-verse", &palette.accent);
    set_bg(buffer, "search-hit", &palette.yellow, 0.45);
    set_bg(buffer, "hl-gold", &palette.yellow, 0.34);
    set_bg(buffer, "hl-green", &palette.green, 0.30);
    set_bg(buffer, "hl-blue", &palette.blue, 0.30);
    set_bg(buffer, "hl-rose", &palette.red, 0.26);
    set_underline(buffer, "user-bookmark", &palette.orange);
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Publish {
    Changed,
    Same,
    Wait,
}

fn publish(provider: &gtk::CssProvider, sticky: bool) -> Publish {
    let Some(palette) = load(sticky) else {
        return Publish::Wait;
    };
    let mut guard = CURRENT.lock().expect("theme lock");
    if guard.as_ref() == Some(&palette) {
        return Publish::Same;
    }
    let css = if palette.chrome {
        format!("{}{SURFACE_CSS}", chrome_css(&palette))
    } else {
        SURFACE_CSS.to_string()
    };
    let scheme = if palette.chrome {
        if palette.dark {
            adw::ColorScheme::ForceDark
        } else {
            adw::ColorScheme::ForceLight
        }
    } else {
        adw::ColorScheme::Default
    };
    *guard = Some(palette);
    drop(guard);
    provider.load_from_string(&css);
    let style = adw::StyleManager::default();
    if style.color_scheme() != scheme {
        style.set_color_scheme(scheme);
    }
    Publish::Changed
}

fn load(sticky: bool) -> Option<Palette> {
    if let Some(path) = theme_override_path() {
        if path.is_file() {
            if let Some(palette) = read_palette(&path) {
                return Some(palette);
            }
            if sticky {
                return None;
            }
        }
    }
    let colors = omarchy_colors_file();
    if let Some(palette) = read_palette(&colors) {
        return Some(palette);
    }
    if sticky && omarchy_current_dir().is_dir() {
        if let Some(palette) = CURRENT.lock().expect("theme lock").clone() {
            if palette.chrome {
                return None;
            }
        }
    }
    Some(desktop())
}

fn desktop() -> Palette {
    let style = adw::StyleManager::default();
    let dark = style.is_dark();
    let accent = hex_from_rgba(&style.accent_color_rgba());
    let mut palette = if dark {
        adwaita_dark()
    } else {
        adwaita_light()
    };
    palette.accent = ensure(&accent, &palette.background, MIN_INK);
    palette.accent_fg = on_fill(&palette.accent, &palette.background, &palette.foreground);
    palette.chrome = false;
    palette
}

fn adwaita_dark() -> Palette {
    Palette {
        dark: true,
        chrome: false,
        background: "#241f31".into(),
        foreground: "#ffffff".into(),
        accent: "#1c71d8".into(),
        accent_fg: "#ffffff".into(),
        muted: "#9a9996".into(),
        dim: "#77767b".into(),
        header: "#241f31".into(),
        elevated: "#3d3846".into(),
        yellow: "#e5a50a".into(),
        green: "#57e389".into(),
        blue: "#62a0ea".into(),
        red: "#ed333b".into(),
        orange: "#c64600".into(),
        magenta: "#c061cb".into(),
        destructive_fg: "#ffffff".into(),
        success_fg: "#ffffff".into(),
        warning_fg: "#141416".into(),
    }
}

fn adwaita_light() -> Palette {
    Palette {
        dark: false,
        chrome: false,
        background: "#ffffff".into(),
        foreground: "#1e1e1e".into(),
        accent: "#1c71d8".into(),
        accent_fg: "#ffffff".into(),
        muted: "#5e5c64".into(),
        dim: "#77767b".into(),
        header: "#ffffff".into(),
        elevated: "#f6f5f4".into(),
        yellow: "#e5a50a".into(),
        green: "#2ec27e".into(),
        blue: "#1c71d8".into(),
        red: "#e01b24".into(),
        orange: "#c64600".into(),
        magenta: "#c061cb".into(),
        destructive_fg: "#ffffff".into(),
        success_fg: "#ffffff".into(),
        warning_fg: "#141416".into(),
    }
}

fn omarchy_colors_file() -> PathBuf {
    state_dir().join("omarchy/current/theme/colors.toml")
}

fn omarchy_current_dir() -> PathBuf {
    state_dir().join("omarchy/current")
}

fn state_dir() -> PathBuf {
    dirs::state_dir().unwrap_or_else(|| PathBuf::from(".local/state"))
}

fn theme_override_path() -> Option<PathBuf> {
    let raw = std::env::var_os("BIBLE_APP_THEME")?;
    if raw.is_empty() {
        return None;
    }
    Some(PathBuf::from(raw))
}

fn read_palette(path: &Path) -> Option<Palette> {
    let text = std::fs::read_to_string(path).ok()?;
    palette_from_toml(&text)
}

pub fn palette_from_toml(text: &str) -> Option<Palette> {
    let raw: toml::Value = toml::from_str(text).ok()?;
    let table = raw.as_table()?;
    let background = color_key(table, "background")?;
    let foreground = color_key(table, "foreground")?;
    let accent = color_key(table, "accent")?;
    let dark = match table.get("mode").and_then(toml::Value::as_str) {
        Some("light") => false,
        Some("dark") => true,
        _ => luminance(&background) < 0.4,
    };
    let header = color_key(table, "dark_background").unwrap_or_else(|| background.clone());
    let elevated = color_key(table, "lighter_background").unwrap_or_else(|| background.clone());
    let muted_raw = color_key(table, "muted");
    let dim_raw = color_key(table, "dark_foreground");
    let muted = first_readable(
        &[
            muted_raw.as_deref(),
            dim_raw.as_deref(),
            Some(foreground.as_str()),
        ],
        &background,
        MIN_INK,
    );
    let dim = first_readable(
        &[
            dim_raw.as_deref(),
            muted_raw.as_deref(),
            Some(foreground.as_str()),
        ],
        &background,
        MIN_INK,
    );
    let accent = ensure(&accent, &background, MIN_INK);
    let yellow = color_key(table, "yellow").unwrap_or_else(|| "#e5a50a".into());
    let green = color_key(table, "green").unwrap_or_else(|| "#57e389".into());
    let blue = color_key(table, "blue").unwrap_or_else(|| accent.clone());
    let red = color_key(table, "red").unwrap_or_else(|| "#ed333b".into());
    let orange = color_key(table, "orange").unwrap_or_else(|| red.clone());
    let magenta = color_key(table, "magenta").unwrap_or_else(|| red.clone());
    Some(Palette {
        dark,
        chrome: true,
        background: background.clone(),
        foreground: foreground.clone(),
        accent: accent.clone(),
        accent_fg: on_fill(&accent, &background, &foreground),
        muted,
        dim,
        header,
        elevated,
        yellow: yellow.clone(),
        green: green.clone(),
        blue,
        red: red.clone(),
        orange,
        magenta,
        destructive_fg: on_fill(&red, &background, &foreground),
        success_fg: on_fill(&green, &background, &foreground),
        warning_fg: on_fill(&yellow, &background, &foreground),
    })
}

pub fn chrome_css(palette: &Palette) -> String {
    let shade = if palette.dark { 0.42 } else { 0.14 };
    let border = if palette.dark { 0.16 } else { 0.12 };
    let p = palette;
    format!(
        r#"
@define-color accent_bg_color {accent};
@define-color accent_fg_color {accent_fg};
@define-color accent_color {accent};
@define-color window_bg_color {bg};
@define-color window_fg_color {fg};
@define-color view_bg_color {bg};
@define-color view_fg_color {fg};
@define-color headerbar_bg_color {header};
@define-color headerbar_fg_color {fg};
@define-color headerbar_backdrop_color {header};
@define-color headerbar_shade_color alpha(#000000, {shade:.2});
@define-color headerbar_darker_shade_color alpha(#000000, {shade:.2});
@define-color headerbar_border_color alpha({fg}, {border:.2});
@define-color sidebar_bg_color {header};
@define-color sidebar_fg_color {fg};
@define-color sidebar_backdrop_color {header};
@define-color sidebar_shade_color alpha(#000000, {shade:.2});
@define-color sidebar_border_color alpha({fg}, {border:.2});
@define-color card_bg_color {elevated};
@define-color card_fg_color {fg};
@define-color card_shade_color alpha(#000000, {shade:.2});
@define-color popover_bg_color {elevated};
@define-color popover_fg_color {fg};
@define-color popover_shade_color alpha(#000000, {shade:.2});
@define-color dialog_bg_color {bg};
@define-color dialog_fg_color {fg};
@define-color shade_color alpha(#000000, {shade:.2});
@define-color destructive_bg_color {red};
@define-color destructive_fg_color {destructive_fg};
@define-color success_bg_color {green};
@define-color success_fg_color {success_fg};
@define-color warning_bg_color {yellow};
@define-color warning_fg_color {warning_fg};
@define-color error_bg_color {red};
@define-color error_fg_color {destructive_fg};
@define-color borders alpha({fg}, {border:.2});

window {{
  --accent-bg-color: {accent};
  --accent-fg-color: {accent_fg};
  --accent-color: {accent};
  --window-bg-color: {bg};
  --window-fg-color: {fg};
  --view-bg-color: {bg};
  --view-fg-color: {fg};
  --headerbar-bg-color: {header};
  --headerbar-fg-color: {fg};
  --sidebar-bg-color: {header};
  --sidebar-fg-color: {fg};
  --card-bg-color: {elevated};
  --card-fg-color: {fg};
  --popover-bg-color: {elevated};
  --popover-fg-color: {fg};
  --dialog-bg-color: {bg};
  --dialog-fg-color: {fg};
  --destructive-bg-color: {red};
  --destructive-fg-color: {destructive_fg};
  --success-bg-color: {green};
  --success-fg-color: {success_fg};
  --warning-bg-color: {yellow};
  --warning-fg-color: {warning_fg};
  --error-bg-color: {red};
  --error-fg-color: {destructive_fg};
}}
"#,
        accent = p.accent,
        accent_fg = p.accent_fg,
        bg = p.background,
        fg = p.foreground,
        header = p.header,
        elevated = p.elevated,
        red = p.red,
        green = p.green,
        yellow = p.yellow,
        destructive_fg = p.destructive_fg,
        success_fg = p.success_fg,
        warning_fg = p.warning_fg,
        shade = shade,
        border = border,
    )
}

const SURFACE_CSS: &str = r#"
textview {
  background-color: var(--view-bg-color);
  color: var(--view-fg-color);
}
textview text {
  color: var(--view-fg-color);
}
textview text selection {
  background-color: color-mix(in srgb, var(--accent-bg-color) 32%, transparent);
  color: var(--view-fg-color);
}
"#;

fn arm_directories(reload: &Rc<Reload>) -> Vec<gtk::gio::FileMonitor> {
    let mut monitors = Vec::new();
    let mut dirs = Vec::new();
    let current = omarchy_current_dir();
    if current.is_dir() {
        dirs.push(current);
    }
    if let Some(path) = theme_override_path() {
        if let Some(parent) = path.parent() {
            if parent.is_dir() {
                dirs.push(parent.to_path_buf());
            }
        }
    }
    for dir in dirs {
        let file = gtk::gio::File::for_path(&dir);
        let Ok(monitor) = file.monitor_directory(
            gtk::gio::FileMonitorFlags::WATCH_MOVES,
            None::<&gtk::gio::Cancellable>,
        ) else {
            continue;
        };
        let reload = Rc::clone(reload);
        monitor.connect_changed(move |_, file, other, _| {
            if event_is_theme(file) || other.is_some_and(event_is_theme) {
                reload.kick();
            }
        });
        monitors.push(monitor);
    }
    monitors
}

fn arm_color_file(reload: &Rc<Reload>) {
    // In-place edits. Directory monitors catch `omarchy theme set` replacing the folder.
    // The override wins when it is set; otherwise watch the Omarchy file.
    let path = if let Some(path) = theme_override_path().filter(|path| path.is_file()) {
        Some(path)
    } else {
        let omarchy = omarchy_colors_file();
        if omarchy.is_file() {
            Some(omarchy)
        } else {
            None
        }
    };
    let Some(path) = path else {
        reload.slot.borrow_mut().take();
        return;
    };
    let file = gtk::gio::File::for_path(&path);
    let Ok(monitor) = file.monitor_file(
        gtk::gio::FileMonitorFlags::NONE,
        None::<&gtk::gio::Cancellable>,
    ) else {
        reload.slot.borrow_mut().take();
        return;
    };
    let kick = Rc::clone(reload);
    monitor.connect_changed(move |_, _, _, event| {
        if event == gtk::gio::FileMonitorEvent::Changed
            || event == gtk::gio::FileMonitorEvent::ChangesDoneHint
            || event == gtk::gio::FileMonitorEvent::Created
            || event == gtk::gio::FileMonitorEvent::Deleted
        {
            kick.kick();
        }
    });
    *reload.slot.borrow_mut() = Some(monitor);
}

fn event_is_theme(file: &gtk::gio::File) -> bool {
    let Some(path) = file.path() else {
        return false;
    };
    if theme_override_path().is_some_and(|override_path| path == override_path) {
        return true;
    }
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    name == "colors.toml" || name == "theme" || name == "theme.name"
}

fn set_fg(buffer: &gtk::TextBuffer, name: &str, color: &str) {
    let Some(tag) = buffer.tag_table().lookup(name) else {
        return;
    };
    tag.set_foreground(Some(color));
}

fn set_bg(buffer: &gtk::TextBuffer, name: &str, hex: &str, alpha: f32) {
    let Some(tag) = buffer.tag_table().lookup(name) else {
        return;
    };
    let Ok(mut color) = gtk::gdk::RGBA::parse(hex) else {
        return;
    };
    color.set_alpha(alpha);
    tag.set_background_rgba(Some(&color));
}

fn set_underline(buffer: &gtk::TextBuffer, name: &str, hex: &str) {
    let Some(tag) = buffer.tag_table().lookup(name) else {
        return;
    };
    if let Ok(color) = gtk::gdk::RGBA::parse(hex) {
        tag.set_underline_rgba(Some(&color));
    }
}

fn color_key(table: &toml::map::Map<String, toml::Value>, key: &str) -> Option<String> {
    let raw = table.get(key)?.as_str()?;
    normalize_hex(raw)
}

fn normalize_hex(hex: &str) -> Option<String> {
    let (r, g, b) = hex_rgb(hex)?;
    Some(format!("#{r:02x}{g:02x}{b:02x}"))
}

fn hex_rgb(hex: &str) -> Option<(u8, u8, u8)> {
    let s = hex.trim().trim_start_matches('#');
    let expanded;
    let digits = if s.len() == 3 {
        expanded = s.chars().flat_map(|c| [c, c]).collect::<String>();
        expanded.as_str()
    } else {
        s
    };
    if digits.len() != 6 || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let n = u32::from_str_radix(digits, 16).ok()?;
    Some((
        ((n >> 16) & 0xff) as u8,
        ((n >> 8) & 0xff) as u8,
        (n & 0xff) as u8,
    ))
}

fn hex_from_rgba(color: &gtk::gdk::RGBA) -> String {
    let channel = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!(
        "#{:02x}{:02x}{:02x}",
        channel(color.red()),
        channel(color.green()),
        channel(color.blue())
    )
}

fn channel_linear(c: u8) -> f32 {
    let s = f32::from(c) / 255.0;
    if s <= 0.04045 {
        s / 12.92
    } else {
        ((s + 0.055) / 1.055).powf(2.4)
    }
}

fn luminance(hex: &str) -> f32 {
    let (r, g, b) = hex_rgb(hex).unwrap_or((0, 0, 0));
    0.2126 * channel_linear(r) + 0.7152 * channel_linear(g) + 0.0722 * channel_linear(b)
}

fn contrast(a: &str, b: &str) -> f32 {
    let l1 = luminance(a);
    let l2 = luminance(b);
    let (hi, lo) = if l1 > l2 { (l1, l2) } else { (l2, l1) };
    (hi + 0.05) / (lo + 0.05)
}

fn ink_on(bg: &str) -> String {
    if contrast("#f4f4f5", bg) >= contrast("#141416", bg) {
        "#f4f4f5".to_string()
    } else {
        "#141416".to_string()
    }
}

fn on_fill(fill: &str, prefer_a: &str, prefer_b: &str) -> String {
    let a = contrast(prefer_a, fill);
    let b = contrast(prefer_b, fill);
    if a >= MIN_FILL || b >= MIN_FILL {
        if a >= b { prefer_a } else { prefer_b }.to_string()
    } else {
        ink_on(fill)
    }
}

fn ensure(fg: &str, bg: &str, min: f32) -> String {
    if contrast(fg, bg) >= min {
        return fg.to_string();
    }
    let alt = ink_on(bg);
    if contrast(&alt, bg) >= min {
        alt
    } else {
        fg.to_string()
    }
}

fn first_readable(candidates: &[Option<&str>], bg: &str, min: f32) -> String {
    for candidate in candidates.iter().flatten() {
        if contrast(candidate, bg) >= min {
            return (*candidate).to_string();
        }
    }
    ink_on(bg)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CATPPUCCIN: &str = r##"
mode = "dark"
accent = "#89b4fa"
background = "#1e1e2e"
dark_background = "#161622"
lighter_background = "#313244"
foreground = "#cdd6f4"
dark_foreground = "#6c7086"
muted = "#585b70"
yellow = "#f9e2af"
green = "#a6e3a1"
blue = "#89b4fa"
red = "#f38ba8"
orange = "#f6b6ab"
magenta = "#f5c2e7"
"##;

    const WHITE: &str = r##"
mode = "light"
accent = "#6e6e6e"
background = "#ffffff"
foreground = "#000000"
dark_foreground = "#c0c0c0"
muted = "#808080"
dark_background = "#f5f5f5"
lighter_background = "#c0c0c0"
red = "#2a2a2a"
yellow = "#4a4a4a"
green = "#3a3a3a"
blue = "#1a1a1a"
"##;

    #[test]
    fn catppuccin_palette_recolors_chrome_and_keeps_accent_readable() {
        let palette = palette_from_toml(CATPPUCCIN).unwrap();
        assert!(palette.dark);
        assert!(palette.chrome);
        assert_eq!(palette.background, "#1e1e2e");
        assert_eq!(palette.accent, "#89b4fa");
        assert_eq!(palette.header, "#161622");
        assert_eq!(palette.elevated, "#313244");
        assert!(contrast(&palette.accent_fg, &palette.accent) >= MIN_FILL);
        assert!(contrast(&palette.muted, &palette.background) >= MIN_INK);
        let css = chrome_css(&palette);
        assert!(css.contains("@define-color accent_bg_color #89b4fa;"));
        assert!(css.contains("--window-bg-color: #1e1e2e;"));
        assert!(css.contains("--headerbar-bg-color: #161622;"));
        assert!(css.contains("--popover-bg-color: #313244;"));
    }

    #[test]
    fn light_theme_is_not_dark_and_dim_text_stays_readable() {
        let palette = palette_from_toml(WHITE).unwrap();
        assert!(!palette.dark);
        assert_eq!(palette.muted, "#808080");
        assert_eq!(palette.dim, "#808080");
        assert!(contrast(&palette.dim, &palette.background) >= MIN_INK);
        assert!(contrast(&palette.accent, &palette.background) >= MIN_INK);
    }

    #[test]
    fn missing_accent_is_not_a_palette() {
        let text = "background = \"#111111\"\nforeground = \"#eeeeee\"\n";
        assert!(palette_from_toml(text).is_none());
    }

    #[test]
    fn short_hex_and_missing_optional_colors_expand() {
        let text = r##"
mode = "dark"
accent = "#8af"
background = "#111"
foreground = "#eee"
"##;
        let palette = palette_from_toml(text).unwrap();
        assert_eq!(palette.accent, "#88aaff");
        assert_eq!(palette.background, "#111111");
        assert_eq!(palette.foreground, "#eeeeee");
        assert_eq!(palette.yellow, "#e5a50a");
        assert_eq!(palette.header, "#111111");
    }
}
