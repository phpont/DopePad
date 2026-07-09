//! Murasaki CSS.

pub const MURASAKI_CSS: &str = r#"
window.dopepad-window {
  background-color: #110118;
  color: #fef7e7;
}

.dopepad-window headerbar {
  background-color: alpha(#300443, 0.92);
  color: #fef7e7;
  border-bottom: 1px solid alpha(#50066f, 0.6);
  box-shadow: none;
  min-height: 40px;
}

.dopepad-window headerbar button {
  color: #e8bcfb;
  border-radius: 8px;
}

.dopepad-window headerbar button:hover {
  background-color: #6f099a;
  color: #fef7e7;
}

.dopepad-title {
  color: #e8bcfb;
  font-weight: 600;
  font-size: 0.95em;
  letter-spacing: 0.02em;
}

.dopepad-surface {
  background-color: #110118;
}

.dopepad-editor {
  background-color: #110118;
  color: #fef7e7;
  font-family: "Inter", "Cantarell", "Noto Sans", sans-serif;
  font-size: 15pt;
  line-height: 1.55;
  caret-color: #e8bcfb;
}

.dopepad-editor text {
  background-color: #110118;
  color: #fef7e7;
  padding: 40px 8px 64px 8px;
}

.dopepad-editor:focus {
  outline: none;
  box-shadow: none;
  border: none;
}

.dopepad-scrolled {
  background-color: #110118;
  border: none;
}

.dopepad-status {
  background-color: alpha(#300443, 0.95);
  color: #e8bcfb;
  font-size: 0.85em;
  padding: 6px 16px;
  border-top: 1px solid alpha(#50066f, 0.5);
}

.dopepad-status.saved {
  color: #e8bcfb;
}

.dopepad-status.dirty {
  color: #fef7e7;
}

.dopepad-search-frame {
  background-color: #300443;
  border: 1px solid #50066f;
  border-radius: 12px;
  padding: 12px;
  margin: 24px;
  box-shadow: 0 12px 40px alpha(#000000, 0.45);
}

.dopepad-search-entry {
  background-color: #50066f;
  color: #fef7e7;
  border-radius: 8px;
  padding: 8px 12px;
  border: none;
  margin-bottom: 8px;
}

.dopepad-search-list {
  background-color: transparent;
  color: #fef7e7;
}

.dopepad-search-list row {
  border-radius: 8px;
  padding: 8px 10px;
  margin: 2px 0;
  color: #fef7e7;
}

.dopepad-search-list row:selected,
.dopepad-search-list row:hover {
  background-color: #6f099a;
}

.dopepad-hover-strip {
  min-height: 8px;
  background-color: transparent;
}

.dopepad-find-bar {
  background-color: alpha(#300443, 0.98);
  border-bottom: 1px solid alpha(#50066f, 0.6);
  padding: 8px 12px;
}

.dopepad-find-entry {
  background-color: #50066f;
  color: #fef7e7;
  border-radius: 8px;
  padding: 6px 10px;
}
"#;

pub fn load_css() {
    use gtk::gdk::Display;
    use gtk::CssProvider;
    use gtk::STYLE_PROVIDER_PRIORITY_APPLICATION;

    let provider = CssProvider::new();
    provider.load_from_string(MURASAKI_CSS);

    if let Some(display) = Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}
