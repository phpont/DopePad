use std::env;
use std::fs;
use std::io;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::core::{Command, SearchState, TextBuffer};
use crate::input::map_key_event;
use crate::io::{
    EolStyle, load_document, load_sidecar, save_document, save_sidecar, sidecar_path_for,
};
use crate::ui::{UiModel, draw};

#[derive(Parser, Debug)]
#[command(author, version, about = "DopePad - TUI Notepad")]
struct Cli {
    #[arg(value_name = "FILE")]
    file: Option<PathBuf>,
    #[arg(long)]
    readonly: bool,
    #[arg(long)]
    no_style: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    Edit,
    ReadOnly,
}

#[derive(Debug, Clone)]
pub enum Overlay {
    None,
    Help,
    Search {
        input: String,
        state: SearchState,
    },
    Goto {
        input: String,
    },
    SaveAs {
        filename: String,
        category_index: usize,
    },
    NewFile {
        filename: String,
        category_index: usize,
    },
    NewCategory {
        name: String,
        next: PostCategoryAction,
    },
    ConfirmUnsaved {
        file_name: String,
        pending: PendingAction,
        choice: ConfirmChoice,
    },
    ConfirmDelete {
        file_name: String,
        path: PathBuf,
        choice: ConfirmChoice,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Clone)]
pub enum PendingAction {
    Quit,
    OpenPath(PathBuf),
    OpenNewFileOverlay { preferred_category: Option<usize> },
    DeletePath(PathBuf),
}

#[derive(Debug, Clone)]
pub enum PostCategoryAction {
    None,
    OpenSaveAs { pending: Option<PendingAction> },
    OpenNewFile { preferred_category: Option<usize> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmChoice {
    Yes,
    No,
}

#[derive(Debug, Clone)]
pub enum TreeNodeKind {
    Category,
    File,
    Empty,
}

#[derive(Debug, Clone)]
pub struct TreeNode {
    pub label: String,
    pub kind: TreeNodeKind,
    pub path: Option<PathBuf>,
    pub category_index: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct FileTree {
    pub nodes: Vec<TreeNode>,
    pub selected: usize,
    pub focus: bool,
}

impl FileTree {
    fn new() -> Self {
        Self {
            nodes: Vec::new(),
            selected: 0,
            focus: false,
        }
    }

    fn selected_path(&self) -> Option<PathBuf> {
        self.nodes
            .get(self.selected)
            .and_then(|n| n.path.as_ref().cloned())
    }

    fn selected_category_index(&self) -> Option<usize> {
        self.nodes.get(self.selected).and_then(|n| n.category_index)
    }

    fn select_first_file(&mut self) {
        if let Some((idx, _)) = self
            .nodes
            .iter()
            .enumerate()
            .find(|(_, n)| matches!(n.kind, TreeNodeKind::File))
        {
            self.selected = idx;
        } else {
            self.selected = 0;
        }
    }

    fn move_selection(&mut self, direction: isize) {
        if self.nodes.is_empty() {
            return;
        }

        let len = self.nodes.len();
        let mut idx = self.selected;
        for _ in 0..len {
            idx = if direction < 0 {
                (idx + len - 1) % len
            } else {
                (idx + 1) % len
            };
            if matches!(self.nodes[idx].kind, TreeNodeKind::File) {
                self.selected = idx;
                break;
            }
        }
    }
}

pub struct App {
    pub buffer: TextBuffer,
    pub overlay: Overlay,
    pub mode: AppMode,
    pub eol: EolStyle,
    pub running: bool,
    pub needs_redraw: bool,
    pub no_style: bool,
    pub notes_root: PathBuf,
    pub file_tree: FileTree,
    pub pending_after_save: Option<PendingAction>,
    pub categories: Vec<String>,
}

impl App {
    fn new(mut buffer: TextBuffer, eol: EolStyle, no_style: bool, notes_root: PathBuf) -> Self {
        let mode = if buffer.readonly {
            AppMode::ReadOnly
        } else {
            AppMode::Edit
        };
        buffer.ensure_cursor_visible();

        let mut app = Self {
            buffer,
            overlay: Overlay::None,
            mode,
            eol,
            running: true,
            needs_redraw: true,
            no_style,
            notes_root,
            file_tree: FileTree::new(),
            pending_after_save: None,
            categories: Vec::new(),
        };
        app.refresh_tree();
        app
    }

    fn open_error(&mut self, msg: impl Into<String>) {
        self.overlay = Overlay::Error {
            message: msg.into(),
        };
        self.needs_redraw = true;
    }

    fn current_file_name(&self) -> String {
        self.buffer
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "[No Name]".to_string())
    }

    fn request_unsaved_confirmation(&mut self, pending: PendingAction) {
        self.overlay = Overlay::ConfirmUnsaved {
            file_name: self.current_file_name(),
            pending,
            choice: ConfirmChoice::Yes,
        };
        self.needs_redraw = true;
    }

    fn refresh_tree(&mut self) {
        let selected_before = self.file_tree.selected_path();
        self.refresh_categories();
        let mut nodes = Vec::new();

        for (category_index, category) in self.categories.iter().enumerate() {
            nodes.push(TreeNode {
                label: format!("[{category}]"),
                kind: TreeNodeKind::Category,
                path: None,
                category_index: Some(category_index),
            });

            let dir = self.notes_root.join(category);
            let mut files: Vec<PathBuf> = fs::read_dir(&dir)
                .ok()
                .into_iter()
                .flat_map(|it| it.filter_map(|e| e.ok()))
                .map(|e| e.path())
                .filter(|p| p.is_file() && p.extension().map(|e| e == "txt").unwrap_or(false))
                .collect();
            files.sort();

            if files.is_empty() {
                nodes.push(TreeNode {
                    label: "  (empty)".to_string(),
                    kind: TreeNodeKind::Empty,
                    path: None,
                    category_index: Some(category_index),
                });
            } else {
                for path in files {
                    let file_name = path
                        .file_name()
                        .map(|f| f.to_string_lossy().to_string())
                        .unwrap_or_else(|| "sem_nome.txt".to_string());
                    nodes.push(TreeNode {
                        label: format!("  {file_name}"),
                        kind: TreeNodeKind::File,
                        path: Some(path),
                        category_index: Some(category_index),
                    });
                }
            }
        }

        self.file_tree.nodes = nodes;

        if let Some(prev_path) = selected_before {
            if let Some((idx, _)) = self
                .file_tree
                .nodes
                .iter()
                .enumerate()
                .find(|(_, n)| n.path.as_ref() == Some(&prev_path))
            {
                self.file_tree.selected = idx;
                return;
            }
        }
        self.file_tree.select_first_file();
    }

    fn refresh_categories(&mut self) {
        let mut categories: Vec<String> = fs::read_dir(&self.notes_root)
            .ok()
            .into_iter()
            .flat_map(|it| it.filter_map(|e| e.ok()))
            .filter_map(|entry| {
                let path = entry.path();
                if path.is_dir() {
                    path.file_name().map(|n| n.to_string_lossy().to_string())
                } else {
                    None
                }
            })
            .collect();
        categories.sort_by_key(|s| s.to_lowercase());
        self.categories = categories;
    }

    fn handle_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && key.modifiers.contains(KeyModifiers::SHIFT)
            && matches!(key.code, KeyCode::Char('S') | KeyCode::Char('s'))
        {
            if self.buffer.readonly {
                self.open_error("Readonly mode: save disabled");
            } else {
                self.open_save_as_overlay();
            }
            self.needs_redraw = true;
            return;
        }

        if !matches!(self.overlay, Overlay::None) {
            self.handle_overlay_key(key);
            return;
        }

        if self.file_tree.focus {
            self.handle_tree_key(key);
            return;
        }

        if let Some(cmd) = map_key_event(key, false) {
            self.apply_command(cmd);
        }
    }

    fn open_save_as_overlay(&mut self) {
        self.open_save_as_overlay_with_pending(None);
    }

    fn open_save_as_overlay_with_pending(&mut self, pending: Option<PendingAction>) {
        self.pending_after_save = pending;
        if self.categories.is_empty() {
            self.overlay = Overlay::NewCategory {
                name: String::new(),
                next: PostCategoryAction::OpenSaveAs {
                    pending: self.pending_after_save.take(),
                },
            };
            return;
        }
        let filename = self
            .buffer
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "note.txt".to_string());

        let category_index = self
            .buffer
            .path
            .as_ref()
            .and_then(|p| self.category_index_for_path(p))
            .unwrap_or(0)
            .min(self.categories.len().saturating_sub(1));

        self.overlay = Overlay::SaveAs {
            filename,
            category_index,
        };
    }

    fn open_new_file_overlay(&mut self, preferred_category: Option<usize>) {
        if self.categories.is_empty() {
            self.overlay = Overlay::NewCategory {
                name: String::new(),
                next: PostCategoryAction::OpenNewFile { preferred_category },
            };
            return;
        }
        let category_index = preferred_category
            .or_else(|| {
                self.buffer
                    .path
                    .as_ref()
                    .and_then(|p| self.category_index_for_path(p))
            })
            .unwrap_or(0)
            .min(self.categories.len().saturating_sub(1));
        self.overlay = Overlay::NewFile {
            filename: "new_note.txt".to_string(),
            category_index,
        };
    }

    fn open_new_category_overlay(&mut self, next: PostCategoryAction) {
        self.overlay = Overlay::NewCategory {
            name: String::new(),
            next,
        };
        self.needs_redraw = true;
    }

    fn open_delete_confirmation(&mut self, path: PathBuf) {
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown.txt".to_string());
        self.overlay = Overlay::ConfirmDelete {
            file_name,
            path,
            choice: ConfirmChoice::No,
        };
        self.needs_redraw = true;
    }

    fn category_index_for_path(&self, path: &Path) -> Option<usize> {
        let parent = path.parent()?;
        self.categories
            .iter()
            .position(|c| self.notes_root.join(c).as_path() == parent)
    }

    fn handle_tree_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.file_tree.focus = false,
            KeyCode::Up => self.file_tree.move_selection(-1),
            KeyCode::Down => self.file_tree.move_selection(1),
            KeyCode::Char('n') | KeyCode::Char('N') => {
                if self.buffer.readonly {
                    self.open_error("Readonly mode: cannot create files");
                    return;
                }
                if self.buffer.dirty {
                    self.request_unsaved_confirmation(PendingAction::OpenNewFileOverlay {
                        preferred_category: self.file_tree.selected_category_index(),
                    });
                    return;
                }
                self.open_new_file_overlay(self.file_tree.selected_category_index());
            }
            KeyCode::Char('c') | KeyCode::Char('C') => {
                self.open_new_category_overlay(PostCategoryAction::None);
            }
            KeyCode::Enter => {
                if self.buffer.dirty {
                    if let Some(path) = self.file_tree.selected_path() {
                        self.request_unsaved_confirmation(PendingAction::OpenPath(path));
                    }
                    return;
                }
                if let Some(path) = self.file_tree.selected_path() {
                    if let Err(e) = self.open_document(&path) {
                        self.open_error(format!("Failed to open file: {e:#}"));
                    }
                }
            }
            KeyCode::Delete | KeyCode::Char('d') | KeyCode::Char('D') => {
                if self.buffer.readonly {
                    self.open_error("Readonly mode: cannot delete files");
                    return;
                }
                if let Some(path) = self.file_tree.selected_path() {
                    if self.buffer.dirty && self.buffer.path.as_ref() == Some(&path) {
                        self.request_unsaved_confirmation(PendingAction::DeletePath(path));
                    } else {
                        self.open_delete_confirmation(path);
                    }
                }
            }
            KeyCode::Char('o') | KeyCode::Char('O')
                if key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                self.file_tree.focus = false;
            }
            _ => {}
        }
        self.needs_redraw = true;
    }

    fn handle_overlay_key(&mut self, key: KeyEvent) {
        let current = std::mem::replace(&mut self.overlay, Overlay::None);
        let mut next = current;

        match next {
            Overlay::Help => {
                if matches!(key.code, KeyCode::Esc | KeyCode::Enter) {
                    next = Overlay::None;
                }
            }
            Overlay::Error { .. } => {
                if matches!(key.code, KeyCode::Esc | KeyCode::Enter) {
                    next = Overlay::None;
                }
            }
            Overlay::ConfirmUnsaved {
                file_name,
                pending,
                mut choice,
            } => match key.code {
                KeyCode::Esc => next = Overlay::None,
                KeyCode::Left | KeyCode::Up => {
                    choice = ConfirmChoice::Yes;
                    next = Overlay::ConfirmUnsaved {
                        file_name,
                        pending,
                        choice,
                    };
                }
                KeyCode::Right | KeyCode::Down => {
                    choice = ConfirmChoice::No;
                    next = Overlay::ConfirmUnsaved {
                        file_name,
                        pending,
                        choice,
                    };
                }
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    choice = ConfirmChoice::Yes;
                    next = Overlay::ConfirmUnsaved {
                        file_name,
                        pending,
                        choice,
                    };
                }
                KeyCode::Char('n') | KeyCode::Char('N') => {
                    choice = ConfirmChoice::No;
                    next = Overlay::ConfirmUnsaved {
                        file_name,
                        pending,
                        choice,
                    };
                }
                KeyCode::Enter => {
                    if choice == ConfirmChoice::Yes {
                        if let Some(path) = self.buffer.path.clone() {
                            if let Err(e) = self.persist_to_path(&path) {
                                self.open_error(format!("Save failed: {e:#}"));
                                return;
                            }
                            self.execute_pending_action(pending);
                        } else {
                            self.open_save_as_overlay_with_pending(Some(pending));
                            return;
                        }
                    } else {
                        self.execute_pending_action(pending);
                    }
                    next = Overlay::None;
                }
                _ => {
                    next = Overlay::ConfirmUnsaved {
                        file_name,
                        pending,
                        choice,
                    }
                }
            },
            Overlay::ConfirmDelete {
                file_name,
                path,
                mut choice,
            } => match key.code {
                KeyCode::Esc => next = Overlay::None,
                KeyCode::Left | KeyCode::Up => {
                    choice = ConfirmChoice::Yes;
                    next = Overlay::ConfirmDelete {
                        file_name,
                        path,
                        choice,
                    };
                }
                KeyCode::Right | KeyCode::Down => {
                    choice = ConfirmChoice::No;
                    next = Overlay::ConfirmDelete {
                        file_name,
                        path,
                        choice,
                    };
                }
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    choice = ConfirmChoice::Yes;
                    next = Overlay::ConfirmDelete {
                        file_name,
                        path,
                        choice,
                    };
                }
                KeyCode::Char('n') | KeyCode::Char('N') => {
                    choice = ConfirmChoice::No;
                    next = Overlay::ConfirmDelete {
                        file_name,
                        path,
                        choice,
                    };
                }
                KeyCode::Enter => {
                    if choice == ConfirmChoice::Yes {
                        if let Err(e) = self.delete_note_path(&path) {
                            self.open_error(format!("Delete failed: {e:#}"));
                            return;
                        }
                    }
                    next = Overlay::None;
                }
                _ => {
                    next = Overlay::ConfirmDelete {
                        file_name,
                        path,
                        choice,
                    }
                }
            },
            Overlay::NewCategory {
                mut name,
                next: next_action,
            } => match key.code {
                KeyCode::Esc => next = Overlay::None,
                KeyCode::Backspace => {
                    name.pop();
                    next = Overlay::NewCategory {
                        name,
                        next: next_action,
                    };
                }
                KeyCode::Enter => {
                    let trimmed = name.trim();
                    if trimmed.is_empty() {
                        self.open_error("Category name cannot be empty");
                        return;
                    }
                    if let Err(e) = self.create_category(trimmed) {
                        self.open_error(format!("Category creation failed: {e:#}"));
                        return;
                    }
                    match next_action {
                        PostCategoryAction::None => {
                            self.overlay = Overlay::None;
                        }
                        PostCategoryAction::OpenSaveAs { pending } => {
                            self.open_save_as_overlay_with_pending(pending);
                        }
                        PostCategoryAction::OpenNewFile { preferred_category } => {
                            self.open_new_file_overlay(preferred_category);
                        }
                    }
                    return;
                }
                KeyCode::Char(c)
                    if !key.modifiers.contains(KeyModifiers::CONTROL)
                        && !key.modifiers.contains(KeyModifiers::ALT)
                        && c != '/'
                        && c != '\\' =>
                {
                    name.push(c);
                    next = Overlay::NewCategory {
                        name,
                        next: next_action,
                    };
                }
                _ => {
                    next = Overlay::NewCategory {
                        name,
                        next: next_action,
                    };
                }
            },
            Overlay::Goto { mut input } => match key.code {
                KeyCode::Esc => next = Overlay::None,
                KeyCode::Backspace => {
                    input.pop();
                    next = Overlay::Goto { input };
                }
                KeyCode::Enter => {
                    if let Ok(n) = input.parse::<usize>() {
                        self.buffer.goto_line(n);
                    }
                    next = Overlay::None;
                }
                KeyCode::Char(c) if c.is_ascii_digit() => {
                    input.push(c);
                    next = Overlay::Goto { input };
                }
                _ => next = Overlay::Goto { input },
            },
            Overlay::SaveAs {
                mut filename,
                mut category_index,
            } => match key.code {
                KeyCode::Esc => next = Overlay::None,
                KeyCode::Up => {
                    if !self.categories.is_empty() {
                        if category_index == 0 {
                            category_index = self.categories.len() - 1;
                        } else {
                            category_index -= 1;
                        }
                    }
                    next = Overlay::SaveAs {
                        filename,
                        category_index,
                    };
                }
                KeyCode::Down => {
                    if !self.categories.is_empty() {
                        category_index = (category_index + 1) % self.categories.len();
                    }
                    next = Overlay::SaveAs {
                        filename,
                        category_index,
                    };
                }
                KeyCode::Backspace => {
                    filename.pop();
                    next = Overlay::SaveAs {
                        filename,
                        category_index,
                    };
                }
                KeyCode::Enter => {
                    if filename.trim().is_empty() {
                        self.open_error("File name cannot be empty");
                        return;
                    }
                    if let Err(e) = self.save_to_category(&filename, category_index) {
                        self.open_error(format!("Save As failed: {e:#}"));
                        return;
                    }
                    if let Some(pending) = self.pending_after_save.take() {
                        self.execute_pending_action(pending);
                    }
                    next = Overlay::None;
                }
                KeyCode::Char(c)
                    if !key.modifiers.contains(KeyModifiers::CONTROL)
                        && !key.modifiers.contains(KeyModifiers::ALT)
                        && c != '/'
                        && c != '\\' =>
                {
                    filename.push(c);
                    next = Overlay::SaveAs {
                        filename,
                        category_index,
                    };
                }
                _ => {
                    next = Overlay::SaveAs {
                        filename,
                        category_index,
                    }
                }
            },
            Overlay::NewFile {
                mut filename,
                mut category_index,
            } => match key.code {
                KeyCode::Esc => next = Overlay::None,
                KeyCode::Up => {
                    if !self.categories.is_empty() {
                        if category_index == 0 {
                            category_index = self.categories.len() - 1;
                        } else {
                            category_index -= 1;
                        }
                    }
                    next = Overlay::NewFile {
                        filename,
                        category_index,
                    };
                }
                KeyCode::Down => {
                    if !self.categories.is_empty() {
                        category_index = (category_index + 1) % self.categories.len();
                    }
                    next = Overlay::NewFile {
                        filename,
                        category_index,
                    };
                }
                KeyCode::Backspace => {
                    filename.pop();
                    next = Overlay::NewFile {
                        filename,
                        category_index,
                    };
                }
                KeyCode::Enter => {
                    if filename.trim().is_empty() {
                        self.open_error("File name cannot be empty");
                        return;
                    }
                    match self.create_new_file_in_category(&filename, category_index) {
                        Ok(path) => {
                            if let Err(e) = self.open_document(&path) {
                                self.open_error(format!("Failed to open new file: {e:#}"));
                                return;
                            }
                            next = Overlay::None;
                        }
                        Err(e) => {
                            self.open_error(format!("File creation failed: {e:#}"));
                            return;
                        }
                    }
                }
                KeyCode::Char(c)
                    if !key.modifiers.contains(KeyModifiers::CONTROL)
                        && !key.modifiers.contains(KeyModifiers::ALT)
                        && c != '/'
                        && c != '\\' =>
                {
                    filename.push(c);
                    next = Overlay::NewFile {
                        filename,
                        category_index,
                    };
                }
                _ => {
                    next = Overlay::NewFile {
                        filename,
                        category_index,
                    }
                }
            },
            Overlay::Search {
                mut input,
                mut state,
            } => match key.code {
                KeyCode::Esc => next = Overlay::None,
                KeyCode::Backspace => {
                    input.pop();
                    state = self.build_search_state(&input, 0);
                    self.jump_to_search_match(&state);
                    next = Overlay::Search { input, state };
                }
                KeyCode::Enter => {
                    if !state.matches.is_empty() {
                        let curr = state.current.unwrap_or(0);
                        let next_idx = if key.modifiers.contains(KeyModifiers::SHIFT) {
                            if curr == 0 {
                                state.matches.len() - 1
                            } else {
                                curr - 1
                            }
                        } else {
                            (curr + 1) % state.matches.len()
                        };
                        state.current = Some(next_idx);
                        self.jump_to_search_match(&state);
                    }
                    next = Overlay::Search { input, state };
                }
                KeyCode::Char(c)
                    if !key.modifiers.contains(KeyModifiers::CONTROL)
                        && !key.modifiers.contains(KeyModifiers::ALT) =>
                {
                    input.push(c);
                    state = self.build_search_state(&input, 0);
                    self.jump_to_search_match(&state);
                    next = Overlay::Search { input, state };
                }
                _ => next = Overlay::Search { input, state },
            },
            Overlay::None => {}
        }

        self.overlay = next;
        self.needs_redraw = true;
    }

    fn build_search_state(&self, query: &str, current_idx: usize) -> SearchState {
        let matches = self.buffer.find_matches(query);
        let current = if matches.is_empty() {
            None
        } else {
            Some(current_idx.min(matches.len() - 1))
        };
        SearchState {
            query: query.to_string(),
            matches,
            current,
        }
    }

    fn jump_to_search_match(&mut self, state: &SearchState) {
        if let Some(i) = state.current {
            if let Some(&line) = state.matches.get(i) {
                self.buffer.goto_line(line + 1);
            }
        }
    }

    fn execute_pending_action(&mut self, pending: PendingAction) {
        match pending {
            PendingAction::Quit => {
                self.running = false;
            }
            PendingAction::OpenPath(path) => {
                if let Err(e) = self.open_document(&path) {
                    self.open_error(format!("Failed to open file: {e:#}"));
                }
            }
            PendingAction::OpenNewFileOverlay { preferred_category } => {
                self.open_new_file_overlay(preferred_category);
            }
            PendingAction::DeletePath(path) => {
                if let Err(e) = self.delete_note_path(&path) {
                    self.open_error(format!("Delete failed: {e:#}"));
                }
            }
        }
    }

    fn apply_command(&mut self, cmd: Command) {
        match cmd {
            Command::Insert(c) => self.buffer.insert_char(c),
            Command::NewLine => self.buffer.insert_newline(),
            Command::Backspace => self.buffer.backspace(),
            Command::Delete => self.buffer.delete(),
            Command::MoveLeft => self.buffer.move_left(),
            Command::MoveRight => self.buffer.move_right(),
            Command::MoveUp => self.buffer.move_up(),
            Command::MoveDown => self.buffer.move_down(),
            Command::MoveHome => self.buffer.move_home(),
            Command::MoveEnd => self.buffer.move_end(),
            Command::PageUp => self.buffer.page_up(),
            Command::PageDown => self.buffer.page_down(),
            Command::SetLineColor(cid) => {
                if self.buffer.readonly {
                    self.open_error("Readonly mode: cannot modify styles");
                } else {
                    self.buffer.set_current_char_color(Some(cid));
                }
            }
            Command::ResetLineColor => {
                if self.buffer.readonly {
                    self.open_error("Readonly mode: cannot modify styles");
                } else {
                    self.buffer.set_current_char_color(None);
                }
            }
            Command::Save => {
                if self.buffer.readonly {
                    self.open_error("Readonly mode: save disabled");
                } else if let Some(path) = self.buffer.path.clone() {
                    if let Err(e) = self.persist_to_path(&path) {
                        self.open_error(format!("Save failed: {e:#}"));
                    }
                } else {
                    self.open_save_as_overlay();
                }
            }
            Command::Quit => {
                if self.buffer.dirty {
                    self.request_unsaved_confirmation(PendingAction::Quit);
                } else {
                    self.running = false;
                }
            }
            Command::OpenHelp => self.overlay = Overlay::Help,
            Command::OpenSearch => {
                let state = self.build_search_state("", 0);
                self.overlay = Overlay::Search {
                    input: String::new(),
                    state,
                };
            }
            Command::OpenGoto => {
                self.overlay = Overlay::Goto {
                    input: String::new(),
                }
            }
            Command::OpenFileTree => {
                self.refresh_tree();
                self.file_tree.focus = !self.file_tree.focus;
            }
            Command::NewFile => {
                if self.buffer.readonly {
                    self.open_error("Readonly mode: cannot create files");
                } else if self.buffer.dirty {
                    self.request_unsaved_confirmation(PendingAction::OpenNewFileOverlay {
                        preferred_category: None,
                    });
                } else {
                    self.open_new_file_overlay(None);
                }
            }
            _ => {}
        }
        self.needs_redraw = true;
    }

    fn save_to_category(&mut self, filename: &str, category_index: usize) -> Result<()> {
        let mut final_name = filename.trim().to_string();
        if !final_name.ends_with(".txt") {
            final_name.push_str(".txt");
        }
        let category = self
            .categories
            .get(category_index)
            .context("invalid category for save")?;
        let path = self.notes_root.join(category).join(final_name);
        self.persist_to_path(&path)?;
        self.refresh_tree();
        Ok(())
    }

    fn create_new_file_in_category(
        &mut self,
        filename: &str,
        category_index: usize,
    ) -> Result<PathBuf> {
        let mut final_name = filename.trim().to_string();
        if !final_name.ends_with(".txt") {
            final_name.push_str(".txt");
        }
        let category = self
            .categories
            .get(category_index)
            .context("invalid category for new file")?;
        let path = self.notes_root.join(category).join(final_name);
        if path.exists() {
            anyhow::bail!("file already exists: {}", path.display());
        }
        fs::write(&path, "").with_context(|| format!("creating file {}", path.display()))?;
        self.refresh_tree();
        Ok(path)
    }

    fn create_category(&mut self, name: &str) -> Result<()> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            anyhow::bail!("category name cannot be empty");
        }
        if trimmed.contains('/') || trimmed.contains('\\') {
            anyhow::bail!("category name contains invalid path characters");
        }
        let path = self.notes_root.join(trimmed);
        if path.exists() {
            anyhow::bail!("category already exists: {}", trimmed);
        }
        fs::create_dir(&path).with_context(|| format!("creating category {}", trimmed))?;
        self.refresh_tree();
        self.file_tree.focus = true;
        Ok(())
    }

    fn delete_note_path(&mut self, path: &Path) -> Result<()> {
        fs::remove_file(path).with_context(|| format!("deleting file {}", path.display()))?;

        if !self.no_style {
            let sidecar = sidecar_path_for(path);
            match fs::remove_file(&sidecar) {
                Ok(()) => {}
                Err(e) if e.kind() == ErrorKind::NotFound => {}
                Err(e) => {
                    return Err(e)
                        .with_context(|| format!("deleting sidecar {}", sidecar.display()));
                }
            }
        }

        if self.buffer.path.as_deref() == Some(path) {
            let readonly = matches!(self.mode, AppMode::ReadOnly);
            let mut new_buffer = TextBuffer::new(None, readonly);
            new_buffer.set_viewport_size(self.buffer.viewport.width, self.buffer.viewport.height);
            self.buffer = new_buffer;
            self.eol = EolStyle::Lf;
        }

        self.refresh_tree();
        self.file_tree.focus = true;
        Ok(())
    }

    fn persist_to_path(&mut self, path: &Path) -> Result<()> {
        save_document(path, &self.buffer.as_string(), self.eol)
            .with_context(|| format!("saving document to {}", path.display()))?;

        if !self.no_style {
            let sidecar = sidecar_path_for(path);
            save_sidecar(&sidecar, &self.buffer.char_colors)
                .with_context(|| format!("saving sidecar to {}", sidecar.display()))?;
        }

        self.buffer.path = Some(path.to_path_buf());
        self.buffer.mark_saved();
        self.refresh_tree();
        Ok(())
    }

    fn open_document(&mut self, path: &Path) -> Result<()> {
        let doc =
            load_document(path).with_context(|| format!("loading file {}", path.display()))?;
        let readonly = matches!(self.mode, AppMode::ReadOnly);
        let mut buffer = TextBuffer::from_text(doc.text, Some(path.to_path_buf()), readonly);
        if !self.no_style {
            let sidecar_path = sidecar_path_for(path);
            if let Ok(colors) = load_sidecar(&sidecar_path) {
                buffer.set_line_colors(colors);
            }
        }
        buffer.set_viewport_size(self.buffer.viewport.width, self.buffer.viewport.height);
        self.buffer = buffer;
        self.eol = doc.eol;
        self.file_tree.focus = false;
        Ok(())
    }

    fn update_viewport_from_size(&mut self, width: u16, height: u16) {
        let (editor_w, editor_h) = if width >= 100 {
            let sidebar = 68.min(width.saturating_sub(20)).max(28);
            (
                width.saturating_sub(sidebar).saturating_sub(2),
                height.saturating_sub(1).saturating_sub(2),
            )
        } else {
            (
                width.saturating_sub(2),
                height.saturating_sub(2).saturating_sub(2),
            )
        };
        self.buffer
            .set_viewport_size(editor_w.max(1), editor_h.max(1));
    }

    fn file_title(&self) -> String {
        self.buffer
            .path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "[No Name]".to_string())
    }

    fn status_hint(&self) -> String {
        if self.file_tree.focus {
            return "TREE: Up/Down select | Enter open | N new | C category | Del delete | Esc back"
                .to_string();
        }
        if self.buffer.readonly {
            "Ctrl+O Tree | Ctrl+Q Quit | Ctrl+F Search | F1 Help".to_string()
        } else {
            "Ctrl+N New | Ctrl+O Tree | Ctrl+S Save | Ctrl+Shift+S SaveAs | Ctrl+Q Quit".to_string()
        }
    }
}

fn default_notes_root() -> Result<PathBuf> {
    let home = env::var("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home)
        .join(".local")
        .join("share")
        .join("dopepad")
        .join("notes"))
}

fn ensure_notes_root(root: &Path) -> Result<()> {
    fs::create_dir_all(root).with_context(|| format!("creating notes root {}", root.display()))?;
    Ok(())
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    let notes_root = default_notes_root()?;
    ensure_notes_root(&notes_root)?;

    let mut eol = EolStyle::Lf;

    let mut buffer = if let Some(path) = &cli.file {
        if path.exists() {
            let doc =
                load_document(path).with_context(|| format!("loading file {}", path.display()))?;
            eol = doc.eol;
            let mut b = TextBuffer::from_text(doc.text, Some(path.clone()), cli.readonly);
            if !cli.no_style {
                let sidecar_path = sidecar_path_for(path);
                if let Ok(colors) = load_sidecar(&sidecar_path) {
                    b.set_line_colors(colors);
                }
            }
            b
        } else {
            TextBuffer::new(Some(path.clone()), cli.readonly)
        }
    } else {
        TextBuffer::new(None, cli.readonly)
    };

    if cli.readonly {
        buffer.readonly = true;
    }

    let mut app = App::new(buffer, eol, cli.no_style, notes_root);
    let (_guard, mut terminal) = setup_terminal()?;
    let size = terminal.size()?;
    app.update_viewport_from_size(size.width, size.height);

    while app.running {
        if app.needs_redraw {
            terminal.draw(|f| {
                draw(
                    f,
                    UiModel {
                        buffer: &app.buffer,
                        mode: app.mode,
                        overlay: &app.overlay,
                        file_title: app.file_title(),
                        hint: app.status_hint(),
                        no_style: app.no_style,
                        file_tree: &app.file_tree,
                        categories: &app.categories,
                    },
                );
            })?;
            app.needs_redraw = false;
        }

        if event::poll(Duration::from_millis(120))? {
            match event::read()? {
                Event::Key(key) => {
                    app.handle_key(key);
                }
                Event::Resize(w, h) => {
                    app.update_viewport_from_size(w, h);
                    app.needs_redraw = true;
                }
                _ => {}
            }
        }
    }

    terminal.show_cursor().context("show cursor")?;
    Ok(())
}

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            LeaveAlternateScreen,
            crossterm::event::DisableMouseCapture
        );
    }
}

fn setup_terminal() -> Result<(TerminalGuard, Terminal<CrosstermBackend<io::Stdout>>)> {
    enable_raw_mode().context("enabling raw mode")?;
    execute!(
        io::stdout(),
        EnterAlternateScreen,
        crossterm::event::EnableMouseCapture
    )
    .context("enter alternate screen")?;

    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            LeaveAlternateScreen,
            crossterm::event::DisableMouseCapture
        );
        hook(panic_info);
    }));

    let guard = TerminalGuard;
    let backend = CrosstermBackend::new(io::stdout());
    let terminal = Terminal::new(backend).context("creating terminal")?;
    Ok((guard, terminal))
}
