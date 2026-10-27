//! GTK4 / libadwaita UI.

use crate::storage::{self, document_window_title, NoteDocument, NoteKind, NoteListItem};
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
use std::path::Path;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

const WEIGHT_BOLD: i32 = 700;
const WEIGHT_SEMIBOLD: i32 = 600;

type RefreshCallback = Rc<dyn Fn()>;
type RefreshSlot = Rc<RefCell<Option<RefreshCallback>>>;

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
    static SPARE: RefCell<Option<EditorHandle>> = const { RefCell::new(None) };
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

fn reload_theme(app: &adw::Application) {
    let palette = theme::reload_css();

    for window in app.windows() {
        let mut child = window.first_child();

        while let Some(widget) = child {
            let next = widget.next_sibling();
            refresh_widget_theme(&widget, &palette);
            child = next;
        }
    }
}

fn refresh_widget_theme(widget: &gtk::Widget, palette: &theme::Palette) {
    if let Some(view) = widget.downcast_ref::<TextView>() {
        let buffer = view.buffer();
        let table = buffer.tag_table();
        apply_tag_palette(&table, palette);
    }

    let mut child = widget.first_child();

    while let Some(current) = child {
        let next = current.next_sibling();
        refresh_widget_theme(&current, palette);
        child = next;
    }
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
        LaunchAction::ReloadTheme => {
            reload_theme(app);
            0
        }
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
            title: None,
            pinned: false,
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
    OPEN_NOTES.with(|notes| {
        let mut notes = notes.borrow_mut();
        notes.retain(|_, current| current != window);
        notes.insert(path, window.clone());
    });
}

fn build_window(app: &adw::Application, doc: NoteDocument, as_spare: bool) {
    let window = adw::ApplicationWindow::new(app);
    window.set_title(Some(&document_window_title(&doc)));
    window.set_default_size(820, 920);
    window.add_css_class("dopepad-window");

    // The mapped path can change when the user navigates inside a window.
    let closing_window = window.clone();
    window.connect_close_request(move |_| {
        OPEN_NOTES.with(|notes| {
            notes.borrow_mut().retain(|_, current| current != &closing_window);
        });
        glib::Propagation::Proceed
    });

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
    title_label.set_ellipsize(pango::EllipsizeMode::End);
    title_label.set_max_width_chars(26);
    header.set_title_widget(Some(&title_label));

    let btn_daily = gtk::Button::with_label("Daily");
    btn_daily.set_tooltip_text(Some("Open daily note (Ctrl+D)"));
    let btn_new = gtk::Button::with_label("New");
    btn_new.set_tooltip_text(Some("New note (Ctrl+N)"));
    let btn_search = gtk::Button::with_label("Search");
    btn_search.set_tooltip_text(Some("Search notes (Ctrl+P)"));
    let btn_save = gtk::Button::with_label("Save");
    btn_save.set_tooltip_text(Some("Save (Ctrl+S)"));
    let btn_library = gtk::Button::with_label("Notes");
    btn_library.set_tooltip_text(Some("Browse saved notes (Ctrl+B)"));
    let btn_rename = gtk::Button::with_label("Rename");
    btn_rename.set_tooltip_text(Some("Rename current note (Ctrl+R)"));

    header.pack_start(&btn_daily);
    header.pack_start(&btn_new);
    header.pack_end(&btn_save);
    header.pack_end(&btn_rename);
    header.pack_end(&btn_search);
    header.pack_start(&btn_library);

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

    let library = gtk::Box::new(Orientation::Vertical, 0);
    library.add_css_class("dopepad-library");
    library.set_size_request(286, -1);
    library.set_vexpand(true);
    let library_header = gtk::Box::new(Orientation::Horizontal, 8);
    library_header.add_css_class("dopepad-library-header");
    let library_title = gtk::Label::new(Some("Saved notes"));
    library_title.set_hexpand(true);
    library_title.set_xalign(0.0);
    library_title.add_css_class("dopepad-library-title");
    let library_close = gtk::Button::from_icon_name("window-close-symbolic");
    library_close.set_tooltip_text(Some("Close notes (Esc)"));
    library_close.add_css_class("flat");
    library_header.append(&library_title);
    library_header.append(&library_close);
    let library_list = gtk::ListBox::new();
    library_list.set_selection_mode(gtk::SelectionMode::Single);
    library_list.add_css_class("dopepad-search-list");
    let library_scroll = ScrolledWindow::new();
    library_scroll.set_child(Some(&library_list));
    library_scroll.set_vexpand(true);
    library.append(&library_header);
    library.append(&library_scroll);

    let library_revealer = gtk::Revealer::new();
    library_revealer.set_transition_type(gtk::RevealerTransitionType::SlideRight);
    library_revealer.set_transition_duration(180);
    library_revealer.set_halign(gtk::Align::Start);
    library_revealer.set_valign(gtk::Align::Fill);
    library_revealer.set_child(Some(&library));
    library_revealer.set_reveal_child(false);
    let editor_area = gtk::Overlay::new();
    editor_area.set_child(Some(&clamp));
    editor_area.add_overlay(&library_revealer);

    let status = gtk::Label::new(Some(""));
    status.set_xalign(0.0);
    status.add_css_class("dopepad-status");
    status.set_visible(false);

    let search_overlay_box = gtk::Box::new(Orientation::Vertical, 0);
    search_overlay_box.set_halign(gtk::Align::Fill);
    search_overlay_box.set_valign(gtk::Align::Start);
    search_overlay_box.set_hexpand(true);
    search_overlay_box.set_visible(false);
    search_overlay_box.add_css_class("dopepad-search-frame");

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
    search_scroll.set_size_request(-1, 280);

    search_overlay_box.append(&search_entry);
    search_overlay_box.append(&search_scroll);

    let hover_strip = gtk::Box::new(Orientation::Horizontal, 0);
    hover_strip.add_css_class("dopepad-hover-strip");
    hover_strip.set_hexpand(true);

    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&header);
    toolbar_view.add_top_bar(&find_bar);
    toolbar_view.add_top_bar(&hover_strip);
    toolbar_view.set_content(Some(&editor_area));
    toolbar_view.add_bottom_bar(&status);
    toolbar_view.set_reveal_top_bars(false);
    toolbar_view.set_reveal_bottom_bars(false);

    let search_clamp = adw::Clamp::new();
    search_clamp.set_maximum_size(520);
    search_clamp.set_child(Some(&search_overlay_box));
    search_clamp.set_halign(gtk::Align::Fill);
    search_clamp.set_valign(gtk::Align::Start);
    search_clamp.set_hexpand(true);
    search_clamp.set_visible(false);

    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&toolbar_view));
    overlay.add_overlay(&search_clamp);
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

    // Metadata actions and document switches refresh the open library.
    let refresh_slot: RefreshSlot = Rc::new(RefCell::new(None));

    let do_save = {
        let buffer = buffer.clone();
        let state = Rc::clone(&state);
        let show_status = Rc::clone(&show_status);
        Rc::new(move || -> bool {
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
                    true
                }
                Err(e) => {
                    show_status(&format!("Save failed: {e}"), true);
                    false
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
        let do_save = Rc::clone(&do_save);
        let refresh_slot = Rc::clone(&refresh_slot);
        Rc::new(move |doc: NoteDocument| {
            if state.borrow().dirty.get() && !do_save() {
                return;
            }
            {
                let st = state.borrow();
                if let Some(id) = st.autosave_source.borrow_mut().take() {
                    id.remove();
                }
                if let Some(id) = st.syntax_source.borrow_mut().take() {
                    id.remove();
                }
                st.suppress_change.set(true);
            }
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

            register_open_note(doc.path.clone(), &window);
            let title = document_window_title(&doc);
            window.set_title(Some(&title));
            title_label.set_text(&title);
            text_view.grab_focus();
            show_status(
                &format!(
                    "Opened {}",
                    doc.path.file_name().and_then(|name| name.to_str()).unwrap_or("note")
                ),
                false,
            );
            if let Some(refresh) = refresh_slot.borrow().as_ref().cloned() {
                glib::idle_add_local_once(move || refresh());
            }
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
    let library_paths: Rc<RefCell<Vec<PathBuf>>> = Rc::new(RefCell::new(Vec::new()));
    let note_index: Rc<RefCell<Vec<NoteListItem>>> = Rc::new(RefCell::new(Vec::new()));

    let on_pin: Rc<dyn Fn(PathBuf)> = {
        let note_index = Rc::clone(&note_index);
        let state = Rc::clone(&state);
        let do_save = Rc::clone(&do_save);
        let show_status = Rc::clone(&show_status);
        let refresh_weak = Rc::downgrade(&refresh_slot);
        Rc::new(move |path: PathBuf| {
            let Some(pinned) = note_index.borrow().iter()
                .find(|item| item.path == path)
                .map(|item| item.pinned) else {
                return;
            };
            let current = state.borrow().path == path;
            if current && state.borrow().dirty.get() && !do_save() {
                return;
            }
            match storage::set_note_pinned(&path, !pinned) {
                Ok(fm) => {
                    if current {
                        state.borrow_mut().frontmatter = fm;
                    }
                    show_status(if pinned { "Unpinned" } else { "Pinned" }, false);
                    if let Some(slot) = refresh_weak.upgrade() {
                        if let Some(refresh) = slot.borrow().as_ref().cloned() {
                            glib::idle_add_local_once(move || refresh());
                        }
                    }
                }
                Err(error) => show_status(&format!("Pin failed: {error}"), true),
            }
        })
    };

    let on_rename: Rc<dyn Fn(PathBuf)> = {
        let window = window.clone();
        let state = Rc::clone(&state);
        let do_save = Rc::clone(&do_save);
        let title_label = title_label.clone();
        let show_status = Rc::clone(&show_status);
        let refresh_weak = Rc::downgrade(&refresh_slot);
        Rc::new(move |path: PathBuf| {
            if !is_managed_path(&path) {
                show_status("Only saved DopePad notes can be renamed", false);
                return;
            }
            let doc = match storage::load_note(&path) {
                Ok(doc) => doc,
                Err(error) => {
                    show_status(&format!("Open failed: {error}"), true);
                    return;
                }
            };
            let dialog = adw::AlertDialog::new(Some("Rename note"), Some("Enter a title for this note."));
            dialog.add_response("cancel", "Cancel");
            dialog.add_response("save", "Save");
            dialog.set_default_response(Some("save"));
            dialog.set_close_response("cancel");
            dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);
            let entry = gtk::Entry::new();
            entry.set_max_length(120);
            entry.set_text(doc.frontmatter.title.as_deref().unwrap_or(""));
            entry.set_placeholder_text(Some("Note title"));
            dialog.set_extra_child(Some(&entry));

            let state = Rc::clone(&state);
            let do_save = Rc::clone(&do_save);
            let window = window.clone();
            let present_window = window.clone();
            let title_label = title_label.clone();
            let show_status = Rc::clone(&show_status);
            let refresh_weak = refresh_weak.clone();
            dialog.connect_response(Some("save"), move |_, _| {
                let current = state.borrow().path == path;
                if current && state.borrow().dirty.get() && !do_save() {
                    return;
                }
                match storage::rename_note(&path, entry.text().as_str()) {
                    Ok(fm) => {
                        if current {
                            state.borrow_mut().frontmatter = fm;
                            if let Ok(doc) = storage::load_note(&path) {
                                let title = document_window_title(&doc);
                                window.set_title(Some(&title));
                                title_label.set_text(&title);
                            }
                        }
                        show_status("Renamed", false);
                        if let Some(slot) = refresh_weak.upgrade() {
                            if let Some(refresh) = slot.borrow().as_ref().cloned() {
                                glib::idle_add_local_once(move || refresh());
                            }
                        }
                    }
                    Err(error) => show_status(&format!("Rename failed: {error}"), true),
                }
            });
            dialog.present(Some(&present_window));
        })
    };

    let populate_search: Rc<dyn Fn()> = {
        let search_list = search_list.clone();
        let note_paths = Rc::clone(&note_paths);
        let search_entry = search_entry.clone();
        let note_index = Rc::clone(&note_index);
        let on_pin = Rc::clone(&on_pin);
        let on_rename = Rc::clone(&on_rename);
        Rc::new(move || {
            let filter = search_entry.text().trim().to_lowercase();
            let items: Vec<_> = note_index.borrow().iter()
                .filter(|item| {
                    filter.is_empty()
                        || item.label.to_lowercase().contains(&filter)
                        || item.search_content.contains(&filter)
                })
                .map(|item| {
                    let mut result = item.clone();
                    if !filter.is_empty() {
                        if let Some(line) = item.body.lines().find(|line| line.to_lowercase().contains(&filter)) {
                            result.preview = line.trim().chars().take(120).collect();
                        }
                    }
                    result
                })
                .collect();
            fill_note_list(&search_list, &note_paths, &items, false, &on_pin, &on_rename);
        })
    };

    let populate_library: Rc<dyn Fn()> = {
        let library_list = library_list.clone();
        let library_paths = Rc::clone(&library_paths);
        let note_index = Rc::clone(&note_index);
        let on_pin = Rc::clone(&on_pin);
        let on_rename = Rc::clone(&on_rename);
        Rc::new(move || {
            fill_note_list(&library_list, &library_paths, &note_index.borrow(), true, &on_pin, &on_rename);
        })
    };

    let refresh_index: Rc<dyn Fn()> = {
        let note_index = Rc::clone(&note_index);
        let populate_search = Rc::clone(&populate_search);
        let populate_library = Rc::clone(&populate_library);
        let show_status = Rc::clone(&show_status);
        Rc::new(move || match storage::list_notes() {
            Ok(items) => {
                *note_index.borrow_mut() = items;
                populate_search();
                populate_library();
            }
            Err(error) => show_status(&format!("Notes failed: {error}"), true),
        })
    };
    *refresh_slot.borrow_mut() = Some(Rc::clone(&refresh_index));

    let close_search = {
        let search_overlay_box = search_overlay_box.clone();
        let search_clamp = search_clamp.clone();
        let text_view = text_view.clone();
        Rc::new(move || {
            search_overlay_box.set_visible(false);
            search_clamp.set_visible(false);
            text_view.grab_focus();
        })
    };

    let open_search = {
        let search_overlay_box = search_overlay_box.clone();
        let search_clamp = search_clamp.clone();
        let search_entry = search_entry.clone();
        let refresh_index = Rc::clone(&refresh_index);
        let reveal_header = Rc::clone(&reveal_header);
        Rc::new(move || {
            reveal_header(true);
            refresh_index();
            search_overlay_box.set_visible(true);
            search_clamp.set_visible(true);
            search_entry.grab_focus();
        })
    };

    let toggle_library = {
        let library_revealer = library_revealer.clone();
        let refresh_index = Rc::clone(&refresh_index);
        Rc::new(move || {
            let show = !library_revealer.reveals_child();
            if show {
                refresh_index();
            }
            library_revealer.set_reveal_child(show);
        })
    };

    let close_library = {
        let library_revealer = library_revealer.clone();
        let text_view = text_view.clone();
        Rc::new(move || {
            library_revealer.set_reveal_child(false);
            text_view.grab_focus();
        })
    };
    library_close.connect_clicked({
        let close_library = Rc::clone(&close_library);
        move |_| close_library()
    });
    {
        let source: Rc<RefCell<Option<SourceId>>> = Rc::new(RefCell::new(None));
        search_entry.connect_changed({
            let populate_search = Rc::clone(&populate_search);
            let source = Rc::clone(&source);
            move |_| {
                if let Some(id) = source.borrow_mut().take() {
                    id.remove();
                }
                let populate_search = Rc::clone(&populate_search);
                let source_for_timeout = Rc::clone(&source);
                let id = glib::timeout_add_local_once(Duration::from_millis(90), move || {
                    populate_search();
                    source_for_timeout.borrow_mut().take();
                });
                *source.borrow_mut() = Some(id);
            }
        });
    }

    for (list, paths, close_after_open) in [
        (search_list.clone(), Rc::clone(&note_paths), true),
        (library_list.clone(), Rc::clone(&library_paths), false),
    ] {
        let load_document = Rc::clone(&load_document);
        let close_search = Rc::clone(&close_search);
        let show_status = Rc::clone(&show_status);
        list.connect_row_activated(move |_, row| {
            let index = row.index() as usize;
            if let Some(path) = paths.borrow().get(index).cloned() {
                match storage::load_note(&path) {
                    Ok(doc) => {
                        load_document(doc);
                        if close_after_open {
                            close_search();
                        }
                    }
                    Err(error) => show_status(&format!("Open failed: {error}"), true),
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
        move |_| { do_save(); }
    });
    btn_search.connect_clicked({
        let open_search = Rc::clone(&open_search);
        move |_| open_search()
    });
    btn_library.connect_clicked({
        let toggle_library = Rc::clone(&toggle_library);
        move |_| toggle_library()
    });
    btn_rename.connect_clicked({
        let on_rename = Rc::clone(&on_rename);
        let state = Rc::clone(&state);
        move |_| on_rename(state.borrow().path.clone())
    });

    {
        let do_save = Rc::clone(&do_save);
        let open_new = Rc::clone(&open_new);
        let open_daily = Rc::clone(&open_daily);
        let open_search = Rc::clone(&open_search);
        let open_find = Rc::clone(&open_find);
        let toggle_library = Rc::clone(&toggle_library);
        let close_library = Rc::clone(&close_library);
        let on_rename = Rc::clone(&on_rename);
        let editor_state = Rc::clone(&state);
        let library_revealer = library_revealer.clone();
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
            if ctrl && keyval == Key::b {
                toggle_library();
                return glib::Propagation::Stop;
            }
            if ctrl && keyval == Key::r {
                on_rename(editor_state.borrow().path.clone());
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
                if library_revealer.reveals_child() {
                    close_library();
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

/// Both search and the library use the same row rendering and path mapping.
fn fill_note_list(
    list: &gtk::ListBox,
    paths: &Rc<RefCell<Vec<PathBuf>>>,
    items: &[NoteListItem],
    show_actions: bool,
    on_pin: &Rc<dyn Fn(PathBuf)>,
    on_rename: &Rc<dyn Fn(PathBuf)>,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    paths.borrow_mut().clear();

    for item in items {
        let row = gtk::ListBoxRow::new();
        let content = gtk::Box::new(Orientation::Vertical, 5);

        let title = gtk::Label::new(Some(&item.label));
        title.add_css_class("dopepad-note-title");
        title.set_xalign(0.0);
        title.set_ellipsize(pango::EllipsizeMode::End);
        title.set_max_width_chars(32);
        if item.pinned {
            title.set_tooltip_text(Some("Pinned note"));
        }
        content.append(&title);
        if !item.preview.is_empty() {
            let preview = gtk::Label::new(Some(&item.preview));
            preview.add_css_class("dopepad-note-preview");
            preview.set_xalign(0.0);
            preview.set_ellipsize(pango::EllipsizeMode::End);
            preview.set_max_width_chars(38);
            content.append(&preview);
        }
        if show_actions {
            let actions = gtk::Box::new(Orientation::Horizontal, 6);
            actions.add_css_class("dopepad-note-actions");
            let pin = gtk::Button::with_label(if item.pinned { "Unpin" } else { "Pin" });
            pin.add_css_class("flat");
            pin.set_tooltip_text(Some("Toggle pin"));
            let rename = gtk::Button::with_label("Rename");
            rename.add_css_class("flat");
            let pin_path = item.path.clone();
            let rename_path = item.path.clone();
            let on_pin = Rc::clone(on_pin);
            let on_rename = Rc::clone(on_rename);
            pin.connect_clicked(move |_| on_pin(pin_path.clone()));
            rename.connect_clicked(move |_| on_rename(rename_path.clone()));
            actions.append(&pin);
            actions.append(&rename);
            content.append(&actions);
        }
        row.set_child(Some(&content));
        list.append(&row);
        paths.borrow_mut().push(item.path.clone());
    }
}

fn is_managed_path(path: &Path) -> bool {
    storage::path_suggests_managed(path)
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
        t.set_pixels_above_lines(10);
        t.set_pixels_below_lines(4);
    });

    add_tag(&table, "h2", |t| {
        t.set_weight(WEIGHT_BOLD);
        t.set_scale(1.30);
        t.set_pixels_above_lines(8);
        t.set_pixels_below_lines(3);
    });

    add_tag(&table, "h3", |t| {
        t.set_weight(WEIGHT_SEMIBOLD);
        t.set_scale(1.12);
        t.set_pixels_above_lines(6);
        t.set_pixels_below_lines(2);
    });

    add_tag(&table, "bullet", |t| {
        t.set_left_margin(12);
    });

    add_tag(&table, "task_pending", |t| {
        t.set_left_margin(12);
    });

    add_tag(&table, "task_done", |t| {
        t.set_strikethrough(true);
        t.set_left_margin(12);
    });

    add_tag(&table, "quote", |t| {
        t.set_style(pango::Style::Italic);
        t.set_left_margin(16);
    });

    add_tag(&table, "callout", |t| {
        t.set_weight(WEIGHT_SEMIBOLD);
        t.set_left_margin(12);
    });

    add_tag(&table, "highlight", |_| {});

    add_tag(&table, "bold", |t| {
        t.set_weight(WEIGHT_BOLD);
    });

    add_tag(&table, "code", |t| {
        t.set_family(Some("monospace"));
        t.set_scale(0.95);
    });

    add_tag(&table, "tag", |t| {
        t.set_weight(WEIGHT_SEMIBOLD);
    });

    add_tag(&table, "frontmatter", |t| {
        t.set_family(Some("monospace"));
        t.set_scale(0.85);
    });

    let palette = theme::load_palette();
    apply_tag_palette(&table, &palette);
}

fn apply_tag_palette(table: &gtk::TextTagTable, p: &theme::Palette) {
    if let Some(t) = table.lookup("h1") {
        t.set_foreground(Some(p.primary.as_str()));
    }

    if let Some(t) = table.lookup("h2") {
        t.set_foreground(Some(p.primary.as_str()));
    }

    if let Some(t) = table.lookup("h3") {
        t.set_foreground(Some(p.primary.as_str()));
    }

    if let Some(t) = table.lookup("bullet") {
        t.set_foreground(Some(p.on_surface.as_str()));
    }

    if let Some(t) = table.lookup("task_pending") {
        t.set_foreground(Some(p.on_surface_variant.as_str()));
    }

    if let Some(t) = table.lookup("task_done") {
        t.set_foreground(Some(p.secondary.as_str()));
    }

    if let Some(t) = table.lookup("quote") {
        t.set_foreground(Some(p.tertiary.as_str()));
        t.set_paragraph_background(Some(p.surface_variant.as_str()));
    }

    if let Some(t) = table.lookup("callout") {
        t.set_foreground(Some(p.error.as_str()));
        t.set_paragraph_background(Some(p.surface_variant.as_str()));
    }

    if let Some(t) = table.lookup("highlight") {
        t.set_background(Some(p.primary.as_str()));
        t.set_foreground(Some(p.on_primary.as_str()));
    }

    if let Some(t) = table.lookup("bold") {
        t.set_foreground(Some(p.on_surface.as_str()));
    }

    if let Some(t) = table.lookup("code") {
        t.set_foreground(Some(p.on_surface_variant.as_str()));
        t.set_background(Some(p.surface_variant.as_str()));
    }

    if let Some(t) = table.lookup("tag") {
        t.set_foreground(Some(p.primary.as_str()));
        t.set_background(Some(p.surface_variant.as_str()));
    }

    if let Some(t) = table.lookup("frontmatter") {
        t.set_foreground(Some(p.tertiary.as_str()));
    }
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
