use gtk::gdk::Display;
use gtk::CssProvider;
use gtk::STYLE_PROVIDER_PRIORITY_APPLICATION;
use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;

thread_local! {
    static CSS_PROVIDER: RefCell<Option<CssProvider>> = const { RefCell::new(None) };
}

#[derive(Clone, Debug)]
pub struct Palette {
    pub primary: String,
    pub on_primary: String,
    pub secondary: String,
    pub tertiary: String,
    pub error: String,
    pub surface: String,
    pub on_surface: String,
    pub surface_variant: String,
    pub on_surface_variant: String,
    pub outline: String,
    pub hover: String,
    pub shadow: String,
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            primary: "#8a8a96".into(),
            on_primary: "#101014".into(),
            secondary: "#8a8a96".into(),
            tertiary: "#8a8a96".into(),
            error: "#c65f5f".into(),
            surface: "#101014".into(),
            on_surface: "#f0eef2".into(),
            surface_variant: "#1a1a20".into(),
            on_surface_variant: "#c8c3cc".into(),
            outline: "#44444f".into(),
            hover: "#33333d".into(),
            shadow: "#000000".into(),
        }
    }
}

fn config_path() -> PathBuf {
    if let Some(config) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(config)
            .join("dopepad")
            .join("noctalia-theme.env");
    }

    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home)
            .join(".config")
            .join("dopepad")
            .join("noctalia-theme.env");
    }

    PathBuf::from("noctalia-theme.env")
}

fn valid_hex(value: &str) -> bool {
    let bytes = value.as_bytes();

    bytes.len() == 7
        && bytes.first() == Some(&b'#')
        && bytes[1..]
            .iter()
            .all(|byte| byte.is_ascii_hexdigit())
}

pub fn load_palette() -> Palette {
    let mut palette = Palette::default();

    let Ok(text) = fs::read_to_string(config_path()) else {
        return palette;
    };

    for raw in text.lines() {
        let line = raw.trim();

        if line.is_empty() {
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            continue;
        };

        let key = key.trim();
        let value = value.trim();

        if !valid_hex(value) {
            continue;
        }

        let target = match key {
            "primary" => &mut palette.primary,
            "on_primary" => &mut palette.on_primary,
            "secondary" => &mut palette.secondary,
            "tertiary" => &mut palette.tertiary,
            "error" => &mut palette.error,
            "surface" => &mut palette.surface,
            "on_surface" => &mut palette.on_surface,
            "surface_variant" => &mut palette.surface_variant,
            "on_surface_variant" => &mut palette.on_surface_variant,
            "outline" => &mut palette.outline,
            "hover" => &mut palette.hover,
            "shadow" => &mut palette.shadow,
            _ => continue,
        };

        *target = value.to_ascii_lowercase();
    }

    palette
}

fn build_css(p: &Palette) -> String {
    format!(
        r#"
window.dopepad-window {{
  background-color: {surface};
  color: {on_surface};
}}

.dopepad-window headerbar {{
  background-color: alpha({surface_variant}, 0.92);
  color: {on_surface};
  border-bottom: 1px solid alpha({outline}, 0.65);
  box-shadow: none;
  min-height: 40px;
}}

.dopepad-window headerbar button {{
  color: {primary};
  border-radius: 8px;
}}

.dopepad-window headerbar button:hover {{
  background-color: {hover};
  color: {on_surface};
}}

.dopepad-title {{
  color: {primary};
  font-weight: 600;
  font-size: 0.95em;
  letter-spacing: 0.02em;
}}

.dopepad-surface {{
  background-color: {surface};
}}

.dopepad-editor {{
  background-color: {surface};
  color: {on_surface};
  font-family: "Inter", "Cantarell", "Noto Sans", sans-serif;
  font-size: 15pt;
  line-height: 1.55;
  caret-color: {primary};
}}

.dopepad-editor text {{
  background-color: {surface};
  color: {on_surface};
  padding: 40px 8px 64px 8px;
}}

.dopepad-editor:focus {{
  outline: none;
  box-shadow: none;
  border: none;
}}

.dopepad-scrolled {{
  background-color: {surface};
  border: none;
}}

.dopepad-status {{
  background-color: alpha({surface_variant}, 0.95);
  color: {on_surface_variant};
  font-size: 0.85em;
  padding: 6px 16px;
  border-top: 1px solid alpha({outline}, 0.55);
}}

.dopepad-status.saved {{
  color: {secondary};
}}

.dopepad-status.dirty {{
  color: {on_surface};
}}

.dopepad-search-frame {{
  background-color: {surface_variant};
  border: 1px solid {outline};
  border-radius: 12px;
  padding: 12px;
  margin: 24px;
  box-shadow: 0 12px 40px alpha({shadow}, 0.45);
}}

.dopepad-search-entry {{
  background-color: alpha({outline}, 0.50);
  color: {on_surface};
  border-radius: 8px;
  padding: 8px 12px;
  border: none;
  margin-bottom: 8px;
}}

.dopepad-search-list {{
  background-color: transparent;
  color: {on_surface};
}}

.dopepad-search-list row {{
  border-radius: 8px;
  padding: 8px 10px;
  margin: 2px 0;
  color: {on_surface};
}}

.dopepad-search-list row:selected,
.dopepad-search-list row:hover {{
  background-color: {hover};
}}

.dopepad-hover-strip {{
  min-height: 8px;
  background-color: transparent;
}}

.dopepad-find-bar {{
  background-color: alpha({surface_variant}, 0.98);
  border-bottom: 1px solid alpha({outline}, 0.65);
  padding: 8px 12px;
}}

.dopepad-find-entry {{
  background-color: alpha({outline}, 0.50);
  color: {on_surface};
  border-radius: 8px;
  padding: 6px 10px;
}}
"#,
        primary = p.primary,
        secondary = p.secondary,
        surface = p.surface,
        on_surface = p.on_surface,
        surface_variant = p.surface_variant,
        on_surface_variant = p.on_surface_variant,
        outline = p.outline,
        hover = p.hover,
        shadow = p.shadow,
    )
}

pub fn load_css() {
    let _ = reload_css();
}

pub fn reload_css() -> Palette {
    let palette = load_palette();
    let css = build_css(&palette);

    CSS_PROVIDER.with(|slot| {
        let mut slot = slot.borrow_mut();

        if let Some(provider) = slot.as_ref() {
            provider.load_from_string(&css);
            return;
        }

        let provider = CssProvider::new();
        provider.load_from_string(&css);

        if let Some(display) = Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                STYLE_PROVIDER_PRIORITY_APPLICATION,
            );

            *slot = Some(provider);
        }
    });

    palette
}
