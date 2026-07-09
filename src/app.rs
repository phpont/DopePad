//! GTK4 / libadwaita UI.

use crate::storage::{self, document_window_title, NoteDocument, NoteKind};
use crate::syntax;
use crate::theme;
use crate::{Cli, LaunchAction};
use adw::prelude::*;
use clap::Parser;
use gio::ApplicationCommandLine;
use gtk::gdk::Key;
use gtk::glib::{self, SourceId};
use gtk::{
    EventControllerKey, EventControllerMotion, Orientation, ScrolledWindow, TextBuffer, TextTag,
    TextView, WrapMode,
};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

const WEIGHT_BOLD: i32 = 700;
const WEIGHT_SEMIBOLD: i32 = 600;

const APP_ID: &str = "io.github.phpont.DopePad";
const SYNTAX_DEBOUNCE_MS: u64 = 100;
const AUTOSAVE_DEBOUNCE_MS: u64 = 600;
const STATUS_HIDE_MS: u64 = 1800;
const HEADER_HIDE_MS: u64 = 2200;

struct EditorHandle {
    window: adw::ApplicationWindow,
    buffer: TextBuffer,
    state: Rc<RefCell<EditorState>>,
    title_label: gtk::Label,
}

thread_local! {
    static OPEN_NOTES: RefCell<HashMap<PathBuf, adw::ApplicationWindow>> =
        RefCell::new(HashMap::new());
    static SPARE: RefCell<Option<EditorHandle>> = RefCell::new(None);
    static WARMING: Cell<bool> = const { Cell::new(false) };
}

struct EditorState {
    path: PathBuf,
    kind: NoteKind,
    mode: storage::DocumentMode,
    frontmatter: storage::Frontmatter,
    suppress_change: Cell<bool>,
    dirty: Cell<bool>,
    syntax_source: RefCell<Option<SourceId>>,
    autosave_source: RefCell<Option<SourceId>>,
    status_hide_source: RefCell<Option<SourceId>>,
    header_hide_source: RefCell<Option<SourceId>>,
    header_pinned: Cell<bool>,
}

impl EditorState {
    fn from_doc(doc: &NoteDocument) -> Self {
        Self {
            path: doc.path.clone(),
            kind: doc.frontmatter.kind,
            mode: doc.mode,
            frontmatter: doc.frontmatter.clone(),
            suppress_change: Cell::new(false),
            dirty: Cell::new(false),
            syntax_source: RefCell::new(None),
            autosave_source: RefCell::new(None),
            status_hide_source: RefCell::new(None),
            header_hide_source: RefCell::new(None),
            header_pinned: Cell::new(false),
        }
    }
}

/// Run the single-instance GTK application.
pub fn run() -> glib::ExitCode {
    if std::env::var_os("GTK_A11Y").is_none() {
        // SAFETY: before any threads are spawned.
        unsafe {
            std::env::set_var("GTK_A11Y", "none");
        }
    }

    let app = adw::Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();

    let hold_token: Rc<RefCell<Option<gio::ApplicationHoldGuard>>> = Rc::new(RefCell::new(None));

    app.connect_startup(|_| {
        adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);
        theme::load_css();
    });

    {
        let hold_token = Rc::clone(&hold_token);
        app.connect_command_line(move |app, cmdline| {
            handle_command_line(app, cmdline, &hold_token)
        });
    }

    app.run()
}

fn parse_launch_action(cmdline: &ApplicationCommandLine) -> LaunchAction {
    let args = cmdline.arguments();
    let cli = Cli::parse_from(args.iter());
    LaunchAction::from_cli(&cli)
}

fn handle_command_line(
    app: &adw::Application,
    cmdline: &ApplicationCommandLine,
    hold_token: &Rc<RefCell<Option<gio::ApplicationHoldGuard>>>,
) -> i32 {
    let action = parse_launch_action(cmdline);

    match action {
        LaunchAction::Daemon => {
            if hold_token.borrow().is_none() {
                *hold_token.borrow_mut() = Some(app.hold());
            }
            schedule_spare_warm(app);
            0
        }
        LaunchAction::Daily => match storage::open_or_create_daily() {
            Ok(doc) => {
                open_or_focus(app, doc);
                0
            }
            Err(e) => {
                eprintln!("dopepad: daily failed: {e:#}");
                1
            }
        },
        LaunchAction::New => match storage::create_new_note() {
            Ok(doc) => {
                open_or_focus(app, doc);
                0
            }
            Err(e) => {
                eprintln!("dopepad: new note failed: {e:#}");
                1
            }
        },
        LaunchAction::File(path) => {
            let path = if path.is_absolute() {
                path
            } else {
                std::env::current_dir()
                    .map(|cwd| cwd.join(&path))
                    .unwrap_or(path)
            };
            match storage::resolve_open(false, false, Some(&path)) {
                Ok(doc) => {
                    open_or_focus(app, doc);
                    0
                }
                Err(e) => {
                    eprintln!("dopepad: open failed: {e:#}");
                    1
                }
            }
        }
    }
}

fn schedule_spare_warm(app: &adw::Application) {
    if SPARE.with(|s| s.borrow().is_some()) || WARMING.get() {
        return;
    }
    WARMING.set(true);
    let app = app.clone();
    glib::timeout_add_local_once(Duration::from_millis(200), move || {
        warm_spare(&app);
    });
}

fn warm_spare(app: &adw::Application) {
    if SPARE.with(|s| s.borrow().is_some()) {
        WARMING.set(false);
        return;
    }
    let now = storage::now_local();
    let path = std::env::temp_dir().join(format!(
        "dopepad-spare-{}-{}.dpad",
        std::process::id(),
        now.timestamp()
    ));
    let content = storage::note_template(now);
    let doc = NoteDocument {
        path,
        mode: storage::DocumentMode::Managed,
        frontmatter: storage::Frontmatter {
            kind: NoteKind::Note,
            created_at: now,
            updated_at: now,
        },
        content,
    };
    build_window(app, doc, true);
}

fn open_or_focus(app: &adw::Application, doc: NoteDocument) {
    let path = doc.path.clone();
    let existing = OPEN_NOTES.with(|m| m.borrow().get(&path).cloned());
    if let Some(win) = existing {
        if app.windows().into_iter().any(|w| w == win) {
            win.set_opacity(1.0);
            win.set_visible(true);
            win.present();
            return;
        }
        OPEN_NOTES.with(|m| {
            m.borrow_mut().remove(&path);
        });
    }

    if claim_spare(doc.clone()) {
        schedule_spare_warm(app);
        return;
    }

    build_window(app, doc, false);
    schedule_spare_warm(app);
}

fn claim_spare(doc: NoteDocument) -> bool {
    let handle = SPARE.with(|s| s.borrow_mut().take());
    let Some(handle) = handle else {
        return false;
    };
    OPEN_NOTES.with(|m| {
        m.borrow_mut().remove(&handle.state.borrow().path);
    });
    load_into_handle(&handle, &doc);
    register_open_note(doc.path.clone(), &handle.window);
    handle.window.set_opacity(1.0);
    handle.window.set_visible(true);
    handle.window.present();
    true
}

fn load_into_handle(handle: &EditorHandle, doc: &NoteDocument) {
    handle.state.borrow().suppress_change.set(true);
    {
        let mut st = handle.state.borrow_mut();
        st.path = doc.path.clone();
        st.kind = doc.frontmatter.kind;
        st.mode = doc.mode;
        st.frontmatter = doc.frontmatter.clone();
        st.dirty.set(false);
    }
    handle.buffer.set_text(doc.body());
    apply_syntax(&handle.buffer);
    handle.buffer.set_modified(false);
    handle.state.borrow().suppress_change.set(false);

    let title = document_window_title(doc);
    handle.window.set_title(Some(&title));
    handle.title_label.set_text(&title);
}

fn register_open_note(path: PathBuf, window: &adw::ApplicationWindow) {
    OPEN_NOTES.with(|m| {
        m.borrow_mut().insert(path.clone(), window.clone());
    });
    let path_for_close = path;
    window.connect_close_request(move |_| {
        OPEN_NOTES.with(|m| {
            m.borrow_mut().remove(&path_for_close);
        });
        glib::Propagation::Proceed
    });
}

fn build_window(app: &adw::Application, doc: NoteDocument, as_spare: bool) {
    let window = adw::ApplicationWindow::new(app);
    window.set_title(Some(&document_window_title(&doc)));
    window.set_default_size(820, 920);
    window.add_css_class("dopepad-window");

    if !as_spare {
        register_open_note(doc.path.clone(), &window);
    }

    let shell = gtk::Box::new(Orientation::Vertical, 0);
    shell.add_css_class("dopepad-surface");
    shell.set_hexpand(true);
    shell.set_vexpand(true);
    window.set_content(Some(&shell));

    if as_spare {
        window.set_opacity(0.0);
        window.present();
    } else {
        window.present();
    }

    glib::idle_add_local_once(move || {
        populate_editor(window, doc, as_spare);
    });
}

fn populate_editor(window: adw::ApplicationWindow, doc: NoteDocument, as_spare: bool) {
    let state = Rc::new(RefCell::new(EditorState::from_doc(&doc)));

    let header = adw::HeaderBar::new();
    header.set_show_end_title_buttons(true);
    header.set_show_start_title_buttons(true);

    let title_label = gtk::Label::new(Some(&window.title().unwrap_or_default()));
    title_label.add_css_class("dopepad-title");
    header.set_title_widget(Some(&title_label));

    let btn_daily = gtk::Button::with_label("Daily");
    btn_daily.set_tooltip_text(Some("Open daily note (Ctrl+D)"));
    let btn_new = gtk::Button::with_label("New");
    btn_new.set_tooltip_text(Some("New note (Ctrl+N)"));
    let btn_search = gtk::Button::with_label("Search");
    btn_search.set_tooltip_text(Some("Search notes (Ctrl+P)"));
    let btn_save = gtk::Button::with_label("Save");
    btn_save.set_tooltip_text(Some("Save (Ctrl+S)"));

    header.pack_start(&btn_daily);
    header.pack_start(&btn_new);
    header.pack_end(&btn_save);
    header.pack_end(&btn_search);

    let find_bar = gtk::Box::new(Orientation::Horizontal, 8);
    find_bar.add_css_class("dopepad-find-bar");
    find_bar.set_visible(false);
    let find_entry = gtk::Entry::new();
    find_entry.set_placeholder_text(Some("Find in note…"));
    find_entry.set_hexpand(true);
    find_entry.add_css_class("dopepad-find-entry");
    let find_close = gtk::Button::with_label("Close");
    find_close.set_tooltip_text(Some("Close find (Esc)"));
    find_bar.append(&find_entry);
    find_bar.append(&find_close);

    let buffer = TextBuffer::new(None);
    install_tags(&buffer);
    buffer.set_text(doc.body());

    let text_view = TextView::with_buffer(&buffer);
    text_view.set_wrap_mode(WrapMode::WordChar);
    text_view.set_accepts_tab(true);
    text_view.set_top_margin(8);
    text_view.set_bottom_margin(48);
    text_view.set_left_margin(4);
    text_view.set_right_margin(4);
    text_view.set_pixels_above_lines(2);
    text_view.set_pixels_below_lines(2);
    text_view.set_monospace(false);
    text_view.add_css_class("dopepad-editor");
    text_view.set_hexpand(true);
    text_view.set_vexpand(true);

    let scrolled = ScrolledWindow::builder()
        .child(&text_view)
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .propagate_natural_width(true)
        .build();
    scrolled.add_css_class("dopepad-scrolled");
    scrolled.set_vexpand(true);

    let clamp = adw::Clamp::new();
    clamp.set_maximum_size(720);
    clamp.set_tightening_threshold(560);
    clamp.set_child(Some(&scrolled));
    clamp.add_css_class("dopepad-surface");
    clamp.set_vexpand(true);

    let status = gtk::Label::new(Some(""));
    status.set_xalign(0.0);
    status.add_css_class("dopepad-status");
    status.set_visible(false);

    let search_overlay_box = gtk::Box::new(Orientation::Vertical, 0);
    search_overlay_box.set_halign(gtk::Align::Center);
    search_overlay_box.set_valign(gtk::Align::Start);
    search_overlay_box.set_hexpand(true);
    search_overlay_box.set_visible(false);
    search_overlay_box.add_css_class("dopepad-search-frame");
    search_overlay_box.set_size_request(420, -1);

    let search_entry = gtk::Entry::new();
    search_entry.set_placeholder_text(Some("Search notes…"));
    search_entry.add_css_class("dopepad-search-entry");
    search_entry.set_hexpand(true);

    let search_list = gtk::ListBox::new();
    search_list.set_selection_mode(gtk::SelectionMode::Single);
    search_list.add_css_class("dopepad-search-list");
    let search_scroll = ScrolledWindow::builder()
        .child(&search_list)
        .min_content_height(240)
        .max_content_height(360)
        .propagate_natural_height(true)
        .build();
    search_scroll.set_size_request(400, 280);

    search_overlay_box.append(&search_entry);
    search_overlay_box.append(&search_scroll);

    let hover_strip = gtk::Box::new(Orientation::Horizontal, 0);
    hover_strip.add_css_class("dopepad-hover-strip");
    hover_strip.set_hexpand(true);

    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&header);
    toolbar_view.add_top_bar(&find_bar);
    toolbar_view.add_top_bar(&hover_strip);
    toolbar_view.set_content(Some(&clamp));
    toolbar_view.add_bottom_bar(&status);
    toolbar_view.set_reveal_top_bars(false);
    toolbar_view.set_reveal_bottom_bars(false);

    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&toolbar_view));
    overlay.add_overlay(&search_overlay_box);
    search_overlay_box.set_halign(gtk::Align::Center);
    search_overlay_box.set_valign(gtk::Align::Start);
    search_overlay_box.set_margin_top(48);

    window.set_content(Some(&overlay));
    apply_syntax(&buffer);

    let show_status = {
        let status = status.clone();
        let toolbar_view = toolbar_view.clone();
        let state = Rc::clone(&state);
        Rc::new(move |msg: &str, sticky: bool| {
            status.set_text(msg);
            status.set_visible(true);
            toolbar_view.set_reveal_bottom_bars(true);
            if let Some(id) = state.borrow().status_hide_source.borrow_mut().take() {
                id.remove();
            }
            if !sticky {
                let status2 = status.clone();
                let toolbar_view2 = toolbar_view.clone();
                let state2 = Rc::clone(&state);
                let id = glib::timeout_add_local_once(
                    Duration::from_millis(STATUS_HIDE_MS),
                    move || {
                        status2.set_visible(false);
                        toolbar_view2.set_reveal_bottom_bars(false);
                        state2.borrow().status_hide_source.borrow_mut().take();
                    },
                );
                *state.borrow().status_hide_source.borrow_mut() = Some(id);
            }
        })
    };

    let reveal_header = {
        let toolbar_view = toolbar_view.clone();
        let state = Rc::clone(&state);
        Rc::new(move |temporary: bool| {
            toolbar_view.set_reveal_top_bars(true);
            if let Some(id) = state.borrow().header_hide_source.borrow_mut().take() {
                id.remove();
            }
            if temporary && !state.borrow().header_pinned.get() {
                let toolbar_view2 = toolbar_view.clone();
                let state2 = Rc::clone(&state);
                let id = glib::timeout_add_local_once(
                    Duration::from_millis(HEADER_HIDE_MS),
                    move || {
                        if !state2.borrow().header_pinned.get() {
                            toolbar_view2.set_reveal_top_bars(false);
                        }
                        state2.borrow().header_hide_source.borrow_mut().take();
                    },
                );
                *state.borrow().header_hide_source.borrow_mut() = Some(id);
            }
        })
    };

    let hide_header = {
        let toolbar_view = toolbar_view.clone();
        let state = Rc::clone(&state);
        Rc::new(move || {
            if !state.borrow().header_pinned.get() {
                toolbar_view.set_reveal_top_bars(false);
            }
        })
    };

    let do_save = {
        let buffer = buffer.clone();
        let state = Rc::clone(&state);
        let show_status = Rc::clone(&show_status);
        Rc::new(move || {
            if let Some(id) = state.borrow().autosave_source.borrow_mut().take() {
                id.remove();
            }
            let body = buffer_text(&buffer);
            let (path, mode, fm) = {
                let st = state.borrow();
                (st.path.clone(), st.mode, st.frontmatter.clone())
            };
            match storage::save_document(&path, mode, &fm, &body) {
                Ok(updated_fm) => {
                    state.borrow_mut().frontmatter = updated_fm;
                    state.borrow().dirty.set(false);
                    buffer.set_modified(false);
                    show_status("Saved", false);
                }
                Err(e) => {
                    show_status(&format!("Save failed: {e}"), true);
                }
            }
        })
    };

    let load_document = {
        let buffer = buffer.clone();
        let state = Rc::clone(&state);
        let window = window.clone();
        let title_label = title_label.clone();
        let show_status = Rc::clone(&show_status);
        let text_view = text_view.clone();
        Rc::new(move |doc: NoteDocument| {
            if state.borrow().dirty.get() {
                let body = buffer_text(&buffer);
                let (path, mode, fm) = {
                    let st = state.borrow();
                    (st.path.clone(), st.mode, st.frontmatter.clone())
                };
                let _ = storage::save_document(&path, mode, &fm, &body);
            }
            state.borrow().suppress_change.set(true);
            {
                let mut st = state.borrow_mut();
                st.path = doc.path.clone();
                st.kind = doc.frontmatter.kind;
                st.mode = doc.mode;
                st.frontmatter = doc.frontmatter.clone();
                st.dirty.set(false);
            }
            buffer.set_text(doc.body());
            apply_syntax(&buffer);
            buffer.set_modified(false);
            state.borrow().suppress_change.set(false);

            let t = document_window_title(&doc);
            window.set_title(Some(&t));
            title_label.set_text(&t);
            text_view.grab_focus();
            show_status(
                &format!(
                    "Opened {}",
                    doc.path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("note")
                ),
                false,
            );
        })
    };

    let open_daily = {
        let load_document = Rc::clone(&load_document);
        let show_status = Rc::clone(&show_status);
        Rc::new(move || match storage::open_or_create_daily() {
            Ok(doc) => load_document(doc),
            Err(e) => show_status(&format!("Daily failed: {e}"), true),
        })
    };

    let open_new = {
        let load_document = Rc::clone(&load_document);
        let show_status = Rc::clone(&show_status);
        Rc::new(move || match storage::create_new_note() {
            Ok(doc) => load_document(doc),
            Err(e) => show_status(&format!("New note failed: {e}"), true),
        })
    };

    {
        let state = Rc::clone(&state);
        let buffer_for_syntax = buffer.clone();
        let do_save = Rc::clone(&do_save);
        let show_status = Rc::clone(&show_status);
        buffer.connect_changed(move |_| {
            if state.borrow().suppress_change.get() {
                return;
            }
            state.borrow().dirty.set(true);
            show_status("Editing…", true);

            if let Some(id) = state.borrow().syntax_source.borrow_mut().take() {
                id.remove();
            }
            let buf2 = buffer_for_syntax.clone();
            let state2 = Rc::clone(&state);
            let sid = glib::timeout_add_local_once(
                Duration::from_millis(SYNTAX_DEBOUNCE_MS),
                move || {
                    apply_syntax(&buf2);
                    state2.borrow().syntax_source.borrow_mut().take();
                },
            );
            *state.borrow().syntax_source.borrow_mut() = Some(sid);

            if let Some(id) = state.borrow().autosave_source.borrow_mut().take() {
                id.remove();
            }
            let do_save2 = Rc::clone(&do_save);
            let state3 = Rc::clone(&state);
            let aid = glib::timeout_add_local_once(
                Duration::from_millis(AUTOSAVE_DEBOUNCE_MS),
                move || {
                    do_save2();
                    state3.borrow().autosave_source.borrow_mut().take();
                },
            );
            *state.borrow().autosave_source.borrow_mut() = Some(aid);
        });
    }

    {
        let reveal_header = Rc::clone(&reveal_header);
        let motion = EventControllerMotion::new();
        motion.connect_enter(move |_, _, _| {
            reveal_header(true);
        });
        hover_strip.add_controller(motion);
    }
    {
        let reveal_header = Rc::clone(&reveal_header);
        let state_enter = Rc::clone(&state);
        let enter = EventControllerMotion::new();
        enter.connect_enter(move |_, _, _| {
            state_enter.borrow().header_pinned.set(true);
            reveal_header(false);
        });
        let state_leave = Rc::clone(&state);
        let hide_header2 = Rc::clone(&hide_header);
        let leave = EventControllerMotion::new();
        leave.connect_leave(move |_| {
            state_leave.borrow().header_pinned.set(false);
            hide_header2();
        });
        header.add_controller(enter);
        header.add_controller(leave);
    }

    let note_paths: Rc<RefCell<Vec<PathBuf>>> = Rc::new(RefCell::new(Vec::new()));

    let populate_search = {
        let search_list = search_list.clone();
        let note_paths = Rc::clone(&note_paths);
        let search_entry = search_entry.clone();
        Rc::new(move || {
            while let Some(child) = search_list.first_child() {
                search_list.remove(&child);
            }
            note_paths.borrow_mut().clear();
            let filter = search_entry.text().to_ascii_lowercase();
            let items = storage::list_notes().unwrap_or_default();
            for item in items {
                if !filter.is_empty() && !item.label.to_ascii_lowercase().contains(&filter) {
                    continue;
                }
                let row = gtk::ListBoxRow::new();
                let label = gtk::Label::new(Some(&item.label));
                label.set_xalign(0.0);
                label.set_margin_start(8);
                label.set_margin_end(8);
                label.set_margin_top(6);
                label.set_margin_bottom(6);
                row.set_child(Some(&label));
                search_list.append(&row);
                note_paths.borrow_mut().push(item.path);
            }
        })
    };

    let close_search = {
        let search_overlay_box = search_overlay_box.clone();
        let text_view = text_view.clone();
        Rc::new(move || {
            search_overlay_box.set_visible(false);
            text_view.grab_focus();
        })
    };

    let open_search = {
        let search_overlay_box = search_overlay_box.clone();
        let search_entry = search_entry.clone();
        let populate_search = Rc::clone(&populate_search);
        let reveal_header = Rc::clone(&reveal_header);
        Rc::new(move || {
            reveal_header(true);
            populate_search();
            search_overlay_box.set_visible(true);
            search_entry.grab_focus();
        })
    };

    {
        let populate_search = Rc::clone(&populate_search);
        search_entry.connect_changed(move |_| {
            populate_search();
        });
    }

    {
        let load_document = Rc::clone(&load_document);
        let note_paths = Rc::clone(&note_paths);
        let close_search = Rc::clone(&close_search);
        let show_status = Rc::clone(&show_status);
        search_list.connect_row_activated(move |_, row| {
            let idx = row.index() as usize;
            if let Some(path) = note_paths.borrow().get(idx).cloned() {
                match storage::load_note(&path) {
                    Ok(doc) => {
                        load_document(doc);
                        close_search();
                    }
                    Err(e) => show_status(&format!("Open failed: {e}"), true),
                }
            }
        });
    }

    let open_find = {
        let find_bar = find_bar.clone();
        let find_entry = find_entry.clone();
        let toolbar_view = toolbar_view.clone();
        let state = Rc::clone(&state);
        Rc::new(move || {
            state.borrow().header_pinned.set(true);
            toolbar_view.set_reveal_top_bars(true);
            find_bar.set_visible(true);
            find_entry.grab_focus();
        })
    };
    let close_find = {
        let find_bar = find_bar.clone();
        let text_view = text_view.clone();
        let state = Rc::clone(&state);
        let hide_header = Rc::clone(&hide_header);
        Rc::new(move || {
            find_bar.set_visible(false);
            state.borrow().header_pinned.set(false);
            hide_header();
            text_view.grab_focus();
        })
    };

    {
        let buffer = buffer.clone();
        let show_status = Rc::clone(&show_status);
        find_entry.connect_activate(move |entry| {
            let query = entry.text();
            if query.is_empty() {
                return;
            }
            let text = buffer_text(&buffer);
            let start_off = buffer.cursor_position() as usize;
            let chars: Vec<char> = text.chars().collect();
            let q: Vec<char> = query.chars().collect();
            let qlen = q.len();
            if qlen == 0 || chars.len() < qlen {
                return;
            }
            let mut found: Option<usize> = None;
            for start in start_off..chars.len().saturating_sub(qlen) + 1 {
                if chars[start..start + qlen]
                    .iter()
                    .zip(q.iter())
                    .all(|(a, b)| a.eq_ignore_ascii_case(b))
                {
                    found = Some(start);
                    break;
                }
            }
            if found.is_none() {
                for start in 0..start_off.min(chars.len().saturating_sub(qlen) + 1) {
                    if chars[start..start + qlen]
                        .iter()
                        .zip(q.iter())
                        .all(|(a, b)| a.eq_ignore_ascii_case(b))
                    {
                        found = Some(start);
                        break;
                    }
                }
            }
            if let Some(pos) = found {
                let s = buffer.iter_at_offset(pos as i32);
                let e = buffer.iter_at_offset((pos + qlen) as i32);
                buffer.select_range(&s, &e);
                show_status(&format!("Found “{query}”"), false);
            } else {
                show_status("Not found", false);
            }
        });
    }

    find_close.connect_clicked({
        let close_find = Rc::clone(&close_find);
        move |_| close_find()
    });

    btn_daily.connect_clicked({
        let open_daily = Rc::clone(&open_daily);
        move |_| open_daily()
    });
    btn_new.connect_clicked({
        let open_new = Rc::clone(&open_new);
        move |_| open_new()
    });
    btn_save.connect_clicked({
        let do_save = Rc::clone(&do_save);
        move |_| do_save()
    });
    btn_search.connect_clicked({
        let open_search = Rc::clone(&open_search);
        move |_| open_search()
    });

    {
        let do_save = Rc::clone(&do_save);
        let open_new = Rc::clone(&open_new);
        let open_daily = Rc::clone(&open_daily);
        let open_search = Rc::clone(&open_search);
        let open_find = Rc::clone(&open_find);
        let close_search = Rc::clone(&close_search);
        let close_find = Rc::clone(&close_find);
        let reveal_header = Rc::clone(&reveal_header);
        let search_overlay_box = search_overlay_box.clone();
        let find_bar = find_bar.clone();
        let window_for_key = window.clone();

        let key = EventControllerKey::new();
        key.set_propagation_phase(gtk::PropagationPhase::Capture);
        key.connect_key_pressed(move |_, keyval, _keycode, state_mod| {
            let ctrl = state_mod.contains(gtk::gdk::ModifierType::CONTROL_MASK);

            if keyval == Key::Alt_L || keyval == Key::Alt_R {
                reveal_header(true);
            }

            if ctrl && keyval == Key::s {
                do_save();
                return glib::Propagation::Stop;
            }
            if ctrl && keyval == Key::n {
                open_new();
                return glib::Propagation::Stop;
            }
            if ctrl && keyval == Key::d {
                open_daily();
                return glib::Propagation::Stop;
            }
            if ctrl && keyval == Key::p {
                open_search();
                return glib::Propagation::Stop;
            }
            if ctrl && keyval == Key::f {
                open_find();
                return glib::Propagation::Stop;
            }
            if ctrl && keyval == Key::q {
                window_for_key.close();
                return glib::Propagation::Stop;
            }
            if keyval == Key::Escape {
                if search_overlay_box.is_visible() {
                    close_search();
                    return glib::Propagation::Stop;
                }
                if find_bar.is_visible() {
                    close_find();
                    return glib::Propagation::Stop;
                }
            }
            glib::Propagation::Proceed
        });
        window.add_controller(key);
    }

    {
        let state = Rc::clone(&state);
        let buffer = buffer.clone();
        window.connect_close_request(move |_| {
            if state.borrow().dirty.get() {
                let body = buffer_text(&buffer);
                let (path, mode, fm) = {
                    let st = state.borrow();
                    (st.path.clone(), st.mode, st.frontmatter.clone())
                };
                let _ = storage::save_document(&path, mode, &fm, &body);
            }
            glib::Propagation::Proceed
        });
    }

    if as_spare {
        window.set_visible(false);
        window.set_opacity(1.0);
        SPARE.with(|s| {
            *s.borrow_mut() = Some(EditorHandle {
                window: window.clone(),
                buffer: buffer.clone(),
                state: Rc::clone(&state),
                title_label: title_label.clone(),
            });
        });
        WARMING.set(false);
    } else {
        window.present();
        text_view.grab_focus();
    }
}

fn buffer_text(buffer: &TextBuffer) -> String {
    let start = buffer.start_iter();
    let end = buffer.end_iter();
    buffer.text(&start, &end, true).to_string()
}

fn install_tags(buffer: &TextBuffer) {
    let table = buffer.tag_table();

    add_tag(&table, "h1", |t| {
        t.set_weight(WEIGHT_BOLD);
        t.set_scale(1.55);
        t.set_foreground(Some("#e8bcfb"));
        t.set_pixels_above_lines(10);
        t.set_pixels_below_lines(4);
    });
    add_tag(&table, "h2", |t| {
        t.set_weight(WEIGHT_BOLD);
        t.set_scale(1.30);
        t.set_foreground(Some("#e8bcfb"));
        t.set_pixels_above_lines(8);
        t.set_pixels_below_lines(3);
    });
    add_tag(&table, "h3", |t| {
        t.set_weight(WEIGHT_SEMIBOLD);
        t.set_scale(1.12);
        t.set_foreground(Some("#e8bcfb"));
        t.set_pixels_above_lines(6);
        t.set_pixels_below_lines(2);
    });
    add_tag(&table, "bullet", |t| {
        t.set_foreground(Some("#fef7e7"));
        t.set_left_margin(12);
    });
    add_tag(&table, "task_pending", |t| {
        t.set_foreground(Some("#e8bcfb"));
        t.set_left_margin(12);
    });
    add_tag(&table, "task_done", |t| {
        t.set_foreground(Some("#8e0cc6"));
        t.set_strikethrough(true);
        t.set_left_margin(12);
    });
    add_tag(&table, "quote", |t| {
        t.set_foreground(Some("#e8bcfb"));
        t.set_style(pango::Style::Italic);
        t.set_left_margin(16);
        t.set_paragraph_background(Some("#300443"));
    });
    add_tag(&table, "callout", |t| {
        t.set_foreground(Some("#e80e4f"));
        t.set_weight(WEIGHT_SEMIBOLD);
        t.set_left_margin(12);
        t.set_paragraph_background(Some("#300443"));
    });
    add_tag(&table, "highlight", |t| {
        t.set_background(Some("#50066f"));
        t.set_foreground(Some("#fef7e7"));
    });
    add_tag(&table, "bold", |t| {
        t.set_weight(WEIGHT_BOLD);
        t.set_foreground(Some("#fef7e7"));
    });
    add_tag(&table, "code", |t| {
        t.set_family(Some("monospace"));
        t.set_foreground(Some("#e8bcfb"));
        t.set_background(Some("#300443"));
        t.set_scale(0.95);
    });
    add_tag(&table, "tag", |t| {
        t.set_foreground(Some("#e8bcfb"));
        t.set_weight(WEIGHT_SEMIBOLD);
        t.set_background(Some("#50066f"));
    });
    add_tag(&table, "frontmatter", |t| {
        t.set_foreground(Some("#8e0cc6"));
        t.set_family(Some("monospace"));
        t.set_scale(0.85);
    });
}

fn add_tag(table: &gtk::TextTagTable, name: &str, configure: impl FnOnce(&TextTag)) {
    let tag = TextTag::new(Some(name));
    configure(&tag);
    table.add(&tag);
}

fn apply_syntax(buffer: &TextBuffer) {
    let text = buffer_text(buffer);
    let start = buffer.start_iter();
    let end = buffer.end_iter();
    buffer.remove_all_tags(&start, &end);

    for span in syntax::parse_syntax(&text) {
        if span.start >= span.end {
            continue;
        }
        let s = buffer.iter_at_offset(span.start as i32);
        let e = buffer.iter_at_offset(span.end as i32);
        buffer.apply_tag_by_name(syntax::tag_name(span.kind), &s, &e);
    }
}
