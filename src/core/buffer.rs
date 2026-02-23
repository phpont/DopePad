use std::collections::BTreeMap;
use std::path::PathBuf;

use ropey::Rope;

pub type ColorId = u8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    pub line: usize,
    pub col: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Viewport {
    pub top_line: usize,
    pub left_col: usize,
    pub width: u16,
    pub height: u16,
}

#[derive(Debug, Clone, Default)]
pub struct SearchState {
    pub query: String,
    pub matches: Vec<usize>,
    pub current: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct TextBuffer {
    rope: Rope,
    pub cursor: Cursor,
    pub viewport: Viewport,
    preferred_col: usize,
    pub dirty: bool,
    pub readonly: bool,
    pub path: Option<PathBuf>,
    pub char_colors: BTreeMap<usize, ColorId>,
    pub active_color: Option<ColorId>,
}

impl TextBuffer {
    pub fn new(path: Option<PathBuf>, readonly: bool) -> Self {
        Self::from_text(String::new(), path, readonly)
    }

    pub fn from_text(text: String, path: Option<PathBuf>, readonly: bool) -> Self {
        Self {
            rope: Rope::from_str(&text),
            cursor: Cursor { line: 0, col: 0 },
            viewport: Viewport {
                top_line: 0,
                left_col: 0,
                width: 80,
                height: 20,
            },
            preferred_col: 0,
            dirty: false,
            readonly,
            path,
            char_colors: BTreeMap::new(),
            active_color: None,
        }
    }

    pub fn line_count(&self) -> usize {
        self.rope.len_lines().max(1)
    }

    pub fn line_len_chars(&self, line: usize) -> usize {
        if line >= self.line_count() {
            return 0;
        }
        let raw = self.rope.line(line).len_chars();
        if raw > 0 {
            let line_text = self.rope.line(line);
            if line_text.char(raw - 1) == '\n' {
                return raw - 1;
            }
        }
        raw
    }

    pub fn line_text(&self, line: usize) -> String {
        if line >= self.line_count() {
            return String::new();
        }
        let mut s = self.rope.line(line).to_string();
        if s.ends_with('\n') {
            s.pop();
        }
        s
    }

    pub fn as_string(&self) -> String {
        self.rope.to_string()
    }

    fn line_col_to_char_idx(&self, line: usize, col: usize) -> usize {
        let l = line.min(self.line_count().saturating_sub(1));
        let c = col.min(self.line_len_chars(l));
        self.rope.line_to_char(l) + c
    }

    pub fn line_start_char_idx(&self, line: usize) -> usize {
        let l = line.min(self.line_count().saturating_sub(1));
        self.rope.line_to_char(l)
    }

    pub fn cursor_char_index(&self) -> usize {
        self.line_col_to_char_idx(self.cursor.line, self.cursor.col)
    }

    fn clamp_cursor(&mut self) {
        let max_line = self.line_count().saturating_sub(1);
        self.cursor.line = self.cursor.line.min(max_line);
        self.cursor.col = self.cursor.col.min(self.line_len_chars(self.cursor.line));
    }

    pub fn set_viewport_size(&mut self, width: u16, height: u16) {
        self.viewport.width = width.max(1);
        self.viewport.height = height.max(1);
        self.ensure_cursor_visible();
    }

    pub fn ensure_cursor_visible(&mut self) {
        self.clamp_cursor();
        if self.cursor.line < self.viewport.top_line {
            self.viewport.top_line = self.cursor.line;
        }
        let bottom = self
            .viewport
            .top_line
            .saturating_add(self.viewport.height.saturating_sub(1) as usize);
        if self.cursor.line > bottom {
            self.viewport.top_line = self
                .cursor
                .line
                .saturating_sub(self.viewport.height.saturating_sub(1) as usize);
        }

        if self.cursor.col < self.viewport.left_col {
            self.viewport.left_col = self.cursor.col;
        }
        let right = self
            .viewport
            .left_col
            .saturating_add(self.viewport.width.saturating_sub(1) as usize);
        if self.cursor.col > right {
            self.viewport.left_col = self
                .cursor
                .col
                .saturating_sub(self.viewport.width.saturating_sub(1) as usize);
        }
    }

    pub fn move_left(&mut self) {
        if self.cursor.col > 0 {
            self.cursor.col -= 1;
        } else if self.cursor.line > 0 {
            self.cursor.line -= 1;
            self.cursor.col = self.line_len_chars(self.cursor.line);
        }
        self.preferred_col = self.cursor.col;
        self.ensure_cursor_visible();
    }

    pub fn move_right(&mut self) {
        let len = self.line_len_chars(self.cursor.line);
        if self.cursor.col < len {
            self.cursor.col += 1;
        } else if self.cursor.line + 1 < self.line_count() {
            self.cursor.line += 1;
            self.cursor.col = 0;
        }
        self.preferred_col = self.cursor.col;
        self.ensure_cursor_visible();
    }

    pub fn move_up(&mut self) {
        if self.cursor.line > 0 {
            self.cursor.line -= 1;
            self.cursor.col = self
                .preferred_col
                .min(self.line_len_chars(self.cursor.line));
        }
        self.ensure_cursor_visible();
    }

    pub fn move_down(&mut self) {
        if self.cursor.line + 1 < self.line_count() {
            self.cursor.line += 1;
            self.cursor.col = self
                .preferred_col
                .min(self.line_len_chars(self.cursor.line));
        }
        self.ensure_cursor_visible();
    }

    pub fn move_home(&mut self) {
        self.cursor.col = 0;
        self.preferred_col = 0;
        self.ensure_cursor_visible();
    }

    pub fn move_end(&mut self) {
        self.cursor.col = self.line_len_chars(self.cursor.line);
        self.preferred_col = self.cursor.col;
        self.ensure_cursor_visible();
    }

    pub fn page_up(&mut self) {
        let amount = self.viewport.height.saturating_sub(1) as usize;
        self.cursor.line = self.cursor.line.saturating_sub(amount);
        self.cursor.col = self
            .preferred_col
            .min(self.line_len_chars(self.cursor.line));
        self.ensure_cursor_visible();
    }

    pub fn page_down(&mut self) {
        let amount = self.viewport.height.saturating_sub(1) as usize;
        self.cursor.line = (self.cursor.line + amount).min(self.line_count().saturating_sub(1));
        self.cursor.col = self
            .preferred_col
            .min(self.line_len_chars(self.cursor.line));
        self.ensure_cursor_visible();
    }

    pub fn goto_line(&mut self, line_1based: usize) {
        let target = line_1based
            .saturating_sub(1)
            .min(self.line_count().saturating_sub(1));
        self.cursor.line = target;
        self.cursor.col = self.cursor.col.min(self.line_len_chars(target));
        self.preferred_col = self.cursor.col;
        self.ensure_cursor_visible();
    }

    pub fn insert_char(&mut self, c: char) {
        if self.readonly {
            return;
        }
        let idx = self.line_col_to_char_idx(self.cursor.line, self.cursor.col);
        self.rope.insert_char(idx, c);
        self.shift_char_colors_after_insert(idx, 1);
        if let Some(color) = self.active_color {
            self.char_colors.insert(idx, color);
        }
        self.cursor.col += 1;
        self.preferred_col = self.cursor.col;
        self.dirty = true;
        self.ensure_cursor_visible();
    }

    pub fn insert_newline(&mut self) {
        if self.readonly {
            return;
        }
        let idx = self.line_col_to_char_idx(self.cursor.line, self.cursor.col);
        self.rope.insert_char(idx, '\n');
        self.shift_char_colors_after_insert(idx, 1);
        self.cursor.line += 1;
        self.cursor.col = 0;
        self.preferred_col = 0;
        self.dirty = true;
        self.ensure_cursor_visible();
    }

    pub fn backspace(&mut self) {
        if self.readonly {
            return;
        }
        if self.cursor.col > 0 {
            let idx = self.line_col_to_char_idx(self.cursor.line, self.cursor.col);
            self.rope.remove(idx - 1..idx);
            self.shift_char_colors_after_remove(idx - 1, 1);
            self.cursor.col -= 1;
            self.preferred_col = self.cursor.col;
            self.dirty = true;
        } else if self.cursor.line > 0 {
            let prev_len = self.line_len_chars(self.cursor.line - 1);
            let idx = self.line_col_to_char_idx(self.cursor.line, self.cursor.col);
            self.rope.remove(idx - 1..idx);
            self.shift_char_colors_after_remove(idx - 1, 1);
            self.cursor.line -= 1;
            self.cursor.col = prev_len;
            self.preferred_col = self.cursor.col;
            self.dirty = true;
        }
        self.ensure_cursor_visible();
    }

    pub fn delete(&mut self) {
        if self.readonly {
            return;
        }
        let idx = self.line_col_to_char_idx(self.cursor.line, self.cursor.col);
        if idx >= self.rope.len_chars() {
            return;
        }
        self.rope.remove(idx..idx + 1);
        self.shift_char_colors_after_remove(idx, 1);
        self.dirty = true;
        self.ensure_cursor_visible();
    }

    fn shift_char_colors_after_insert(&mut self, at_char: usize, count: usize) {
        if count == 0 {
            return;
        }
        let mut updates = Vec::new();
        for (&idx, &color) in self.char_colors.range(at_char..) {
            updates.push((idx, color));
        }
        for (idx, _) in &updates {
            self.char_colors.remove(idx);
        }
        for (idx, color) in updates {
            self.char_colors.insert(idx + count, color);
        }
    }

    fn shift_char_colors_after_remove(&mut self, at_char: usize, count: usize) {
        if count == 0 {
            return;
        }
        let mut updates = Vec::new();
        let end = at_char + count;
        for (&idx, &color) in self.char_colors.range(at_char..) {
            if idx < end {
                continue;
            }
            updates.push((idx, color));
        }
        for idx in at_char..end {
            self.char_colors.remove(&idx);
        }
        for (idx, _) in &updates {
            self.char_colors.remove(idx);
        }
        for (idx, color) in updates {
            self.char_colors.insert(idx - count, color);
        }
    }

    pub fn set_current_char_color(&mut self, color: Option<ColorId>) {
        self.active_color = color;
        let idx = self.cursor_char_index();
        if idx < self.rope.len_chars() {
            if self.rope.char(idx) != '\n' {
                match color {
                    Some(id) => {
                        self.char_colors.insert(idx, id);
                    }
                    None => {
                        self.char_colors.remove(&idx);
                    }
                }
                self.dirty = true;
            }
        }
    }

    pub fn current_char_color(&self) -> Option<ColorId> {
        let idx = self.cursor_char_index();
        self.char_colors.get(&idx).copied().or(self.active_color)
    }

    pub fn set_char_colors(&mut self, colors: BTreeMap<usize, ColorId>) {
        self.char_colors = colors;
    }

    pub fn char_color(&self, char_idx: usize) -> Option<ColorId> {
        self.char_colors.get(&char_idx).copied()
    }

    pub fn active_color(&self) -> Option<ColorId> {
        self.active_color
    }

    pub fn set_active_color(&mut self, color: Option<ColorId>) {
        self.active_color = color;
    }

    pub fn set_current_line_color(&mut self, color: Option<ColorId>) {
        self.set_current_char_color(color);
    }

    pub fn current_line_color(&self) -> Option<ColorId> {
        self.current_char_color()
    }

    pub fn set_line_colors(&mut self, colors: BTreeMap<usize, ColorId>) {
        self.set_char_colors(colors);
    }

    pub fn line_color(&self, line: usize) -> Option<ColorId> {
        let idx = self.line_col_to_char_idx(line, 0);
        self.char_color(idx)
    }

    pub fn find_matches(&self, query: &str) -> Vec<usize> {
        if query.is_empty() {
            return Vec::new();
        }
        let query_lower = query.to_lowercase();
        let mut out = Vec::new();
        for line in 0..self.line_count() {
            let line_text = self.line_text(line).to_lowercase();
            if line_text.contains(&query_lower) {
                out.push(line);
            }
        }
        out
    }

    pub fn set_text_from_string(&mut self, text: String) {
        self.rope = Rope::from_str(&text);
        self.cursor = Cursor { line: 0, col: 0 };
        self.viewport.top_line = 0;
        self.viewport.left_col = 0;
        self.preferred_col = 0;
        self.char_colors.clear();
        self.active_color = None;
        self.dirty = false;
    }

    pub fn mark_saved(&mut self) {
        self.dirty = false;
    }
}

#[cfg(test)]
mod tests {
    use super::TextBuffer;

    #[test]
    fn insert_and_backspace_work() {
        let mut b = TextBuffer::new(None, false);
        b.insert_char('A');
        b.insert_char('ç');
        b.backspace();
        assert_eq!(b.as_string(), "A");
        assert_eq!(b.cursor.col, 1);
    }

    #[test]
    fn inserted_chars_keep_active_color() {
        let mut b = TextBuffer::new(None, false);
        b.set_active_color(Some(3));
        b.insert_char('a');
        b.insert_char('b');
        assert_eq!(b.char_color(0), Some(3));
        assert_eq!(b.char_color(1), Some(3));
    }

    #[test]
    fn color_map_shifts_after_insert_and_remove() {
        let mut b = TextBuffer::from_text("ab".into(), None, false);
        b.move_right();
        b.set_current_char_color(Some(5));
        assert_eq!(b.char_color(1), Some(5));

        b.move_home();
        b.insert_char('x');
        assert_eq!(b.char_color(2), Some(5));

        b.delete();
        assert_eq!(b.char_color(1), Some(5));
    }

    #[test]
    fn stress_insertions_keep_valid_text() {
        let mut b = TextBuffer::new(None, false);
        for _ in 0..20000 {
            b.insert_char('x');
        }
        b.insert_newline();
        for _ in 0..10000 {
            b.insert_char('y');
        }
        assert!(b.as_string().contains('\n'));
        assert_eq!(b.line_count(), 2);
        assert_eq!(b.line_len_chars(0), 20000);
    }
}
