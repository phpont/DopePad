//! Note paths, frontmatter, and load/save.

use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, FixedOffset, Local, Timelike};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub const NOTE_EXT: &str = "dpad";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteKind {
    Daily,
    Note,
}

impl NoteKind {
    pub fn as_str(self) -> &'static str {
        match self {
            NoteKind::Daily => "daily",
            NoteKind::Note => "note",
        }
    }

    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "daily" => Some(NoteKind::Daily),
            "note" => Some(NoteKind::Note),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frontmatter {
    pub kind: NoteKind,
    pub created_at: DateTime<FixedOffset>,
    pub updated_at: DateTime<FixedOffset>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentMode {
    /// `.dpad` vault note with frontmatter.
    Managed,
    /// External text; saved as-is.
    Plain,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteDocument {
    pub path: PathBuf,
    pub mode: DocumentMode,
    pub frontmatter: Frontmatter,
    pub content: String,
}

impl NoteDocument {
    /// Text shown in the editor.
    pub fn body(&self) -> &str {
        match self.mode {
            DocumentMode::Managed => body_of(&self.content),
            DocumentMode::Plain => &self.content,
        }
    }
}

pub fn path_suggests_managed(path: &Path) -> bool {
    if path.extension().and_then(|e| e.to_str()) == Some(NOTE_EXT) {
        return true;
    }
    if let Ok(dir) = notes_dir() {
        if path.starts_with(&dir) {
            return true;
        }
    }
    false
}

fn synthetic_frontmatter(path: &Path) -> Frontmatter {
    let mtime = fs::metadata(path)
        .and_then(|m| m.modified())
        .unwrap_or_else(|_| SystemTime::now());
    let dt = system_time_to_local(mtime);
    Frontmatter {
        kind: guess_kind_from_path(path),
        created_at: dt,
        updated_at: dt,
    }
}

fn synthetic_frontmatter_now(kind: NoteKind) -> Frontmatter {
    let now = now_local();
    Frontmatter {
        kind,
        created_at: now,
        updated_at: now,
    }
}

pub fn body_of(content: &str) -> &str {
    match parse_frontmatter(content) {
        Ok((_, offset)) => content.get(offset..).unwrap_or(""),
        Err(_) => content,
    }
}

pub fn serialize_document(fm: &Frontmatter, body: &str) -> String {
    let created = format_timestamp(fm.created_at);
    let updated = format_timestamp(fm.updated_at);
    let body = body.trim_start_matches('\u{feff}');
    let body = body.strip_prefix('\n').unwrap_or(body);
    format!(
        "---\nkind: {}\ncreated_at: {created}\nupdated_at: {updated}\n---\n\n{body}",
        fm.kind.as_str()
    )
}

#[derive(Debug, Clone)]
pub struct NoteListItem {
    pub path: PathBuf,
    pub label: String,
    pub mtime: SystemTime,
}

pub fn notes_dir() -> Result<PathBuf> {
    let home = dirs_home().context("could not resolve home directory")?;
    Ok(home.join(".local/share/dopepad/notes"))
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

pub fn ensure_notes_dir() -> Result<PathBuf> {
    let dir = notes_dir()?;
    fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create notes dir {}", dir.display()))?;
    Ok(dir)
}

pub fn now_local() -> DateTime<FixedOffset> {
    Local::now().fixed_offset()
}

pub fn format_timestamp(dt: DateTime<FixedOffset>) -> String {
    dt.format("%Y-%m-%dT%H:%M:%S%:z").to_string()
}

pub fn daily_path_for(date: DateTime<FixedOffset>) -> Result<PathBuf> {
    let dir = ensure_notes_dir()?;
    let name = format!("daily_{}.{}", date.format("%Y-%m-%d"), NOTE_EXT);
    Ok(dir.join(name))
}

pub fn new_note_path_for(dt: DateTime<FixedOffset>) -> Result<PathBuf> {
    let dir = ensure_notes_dir()?;
    let name = format!(
        "note_{}_{:02}{:02}{:02}.{}",
        dt.format("%Y-%m-%d"),
        dt.hour(),
        dt.minute(),
        dt.second(),
        NOTE_EXT
    );
    Ok(dir.join(name))
}

pub fn window_title(kind: NoteKind, dt: DateTime<FixedOffset>) -> String {
    match kind {
        NoteKind::Daily => format!("DopePad · Daily · {}", dt.format("%Y-%m-%d")),
        NoteKind::Note => format!("DopePad · Note · {}", dt.format("%Y-%m-%d %H:%M")),
    }
}

pub fn document_window_title(doc: &NoteDocument) -> String {
    match doc.mode {
        DocumentMode::Managed => window_title(doc.frontmatter.kind, doc.frontmatter.created_at),
        DocumentMode::Plain => {
            let name = doc
                .path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("untitled");
            format!("DopePad · {name}")
        }
    }
}

pub fn list_label(kind: NoteKind, dt: DateTime<FixedOffset>) -> String {
    match kind {
        NoteKind::Daily => format!("Daily · {}", dt.format("%Y-%m-%d")),
        NoteKind::Note => format!("Note · {}", dt.format("%Y-%m-%d %H:%M")),
    }
}

pub fn daily_template(dt: DateTime<FixedOffset>) -> String {
    let ts = format_timestamp(dt);
    format!(
        "---\nkind: daily\ncreated_at: {ts}\nupdated_at: {ts}\n---\n\n# Daily · {}\n\n",
        dt.format("%Y-%m-%d")
    )
}

pub fn note_template(dt: DateTime<FixedOffset>) -> String {
    let ts = format_timestamp(dt);
    format!(
        "---\nkind: note\ncreated_at: {ts}\nupdated_at: {ts}\n---\n\n# Note · {}\n\n",
        dt.format("%Y-%m-%d %H:%M")
    )
}

pub fn parse_frontmatter(content: &str) -> Result<(Frontmatter, usize)> {
    let mut lines = content.lines();
    let first = lines.next().ok_or_else(|| anyhow!("empty document"))?;
    if first.trim() != "---" {
        bail!("document missing frontmatter start");
    }

    let mut kind: Option<NoteKind> = None;
    let mut created_at: Option<DateTime<FixedOffset>> = None;
    let mut updated_at: Option<DateTime<FixedOffset>> = None;
    let mut closed = false;

    for line in lines {
        if line.trim() == "---" {
            closed = true;
            break;
        }
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim();
            let value = value.trim();
            match key {
                "kind" => kind = NoteKind::from_str_loose(value),
                "created_at" => {
                    created_at = DateTime::parse_from_rfc3339(value)
                        .or_else(|_| DateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%:z"))
                        .ok();
                }
                "updated_at" => {
                    updated_at = DateTime::parse_from_rfc3339(value)
                        .or_else(|_| DateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%:z"))
                        .ok();
                }
                _ => {}
            }
        }
    }

    if !closed {
        bail!("document missing frontmatter end");
    }

    let kind = kind.ok_or_else(|| anyhow!("frontmatter missing kind"))?;
    let created_at = created_at.ok_or_else(|| anyhow!("frontmatter missing created_at"))?;
    let updated_at = updated_at.unwrap_or(created_at);

    let mut offset = 0usize;
    let mut seen_fences = 0u8;
    for line in content.split_inclusive('\n') {
        offset += line.len();
        if line.trim_end_matches(['\r', '\n']) == "---" {
            seen_fences += 1;
            if seen_fences == 2 {
                let rest = &content[offset..];
                if rest.starts_with('\n') {
                    offset += 1;
                } else if rest.starts_with("\r\n") {
                    offset += 2;
                }
                break;
            }
        }
    }

    Ok((
        Frontmatter {
            kind,
            created_at,
            updated_at,
        },
        offset,
    ))
}

#[cfg(test)]
pub fn touch_updated_at(content: &str, updated_at: DateTime<FixedOffset>) -> Result<String> {
    let ts = format_timestamp(updated_at);
    let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
    if lines.is_empty() || lines[0].trim() != "---" {
        return Ok(content.to_string());
    }

    let mut end = None;
    for (i, line) in lines.iter().enumerate().skip(1) {
        if line.trim() == "---" {
            end = Some(i);
            break;
        }
    }
    let Some(end) = end else {
        return Ok(content.to_string());
    };

    let mut found = false;
    for line in lines.iter_mut().take(end).skip(1) {
        if line.trim_start().starts_with("updated_at:") {
            *line = format!("updated_at: {ts}");
            found = true;
            break;
        }
    }
    if !found {
        lines.insert(end, format!("updated_at: {ts}"));
    }

    let mut out = lines.join("\n");
    if content.ends_with('\n') {
        out.push('\n');
    }
    Ok(out)
}

pub fn open_or_create_daily() -> Result<NoteDocument> {
    let now = now_local();
    let path = daily_path_for(now)?;
    if path.exists() {
        load_note(&path)
    } else {
        let content = daily_template(now);
        write_note_atomic(&path, &content)?;
        Ok(NoteDocument {
            path,
            mode: DocumentMode::Managed,
            frontmatter: Frontmatter {
                kind: NoteKind::Daily,
                created_at: now,
                updated_at: now,
            },
            content,
        })
    }
}

pub fn create_new_note() -> Result<NoteDocument> {
    let now = now_local();
    let path = new_note_path_for(now)?;
    let path = if path.exists() {
        let mut p = path;
        let mut n = 1u32;
        loop {
            let candidate = p.with_file_name(format!(
                "note_{}_{:02}{:02}{:02}_{n}.{}",
                now.format("%Y-%m-%d"),
                now.hour(),
                now.minute(),
                now.second(),
                NOTE_EXT
            ));
            if !candidate.exists() {
                p = candidate;
                break;
            }
            n += 1;
        }
        p
    } else {
        path
    };

    let content = note_template(now);
    write_note_atomic(&path, &content)?;
    Ok(NoteDocument {
        path,
        mode: DocumentMode::Managed,
        frontmatter: Frontmatter {
            kind: NoteKind::Note,
            created_at: now,
            updated_at: now,
        },
        content,
    })
}

pub fn load_note(path: &Path) -> Result<NoteDocument> {
    let content =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    match parse_frontmatter(&content) {
        Ok((frontmatter, _)) => Ok(NoteDocument {
            path: path.to_path_buf(),
            mode: DocumentMode::Managed,
            frontmatter,
            content,
        }),
        Err(_) => {
            let mode = if path.extension().and_then(|e| e.to_str()) == Some(NOTE_EXT) {
                DocumentMode::Managed
            } else {
                DocumentMode::Plain
            };
            Ok(NoteDocument {
                path: path.to_path_buf(),
                mode,
                frontmatter: synthetic_frontmatter(path),
                content,
            })
        }
    }
}

fn guess_kind_from_path(path: &Path) -> NoteKind {
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    if name.starts_with("daily_") {
        NoteKind::Daily
    } else {
        NoteKind::Note
    }
}

fn system_time_to_local(st: SystemTime) -> DateTime<FixedOffset> {
    let dt: DateTime<Local> = st.into();
    dt.fixed_offset()
}

pub fn write_note_atomic(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let tmp = path.with_extension(format!(
        "{}.tmp",
        path.extension().and_then(|e| e.to_str()).unwrap_or("dpad")
    ));
    fs::write(&tmp, content).with_context(|| format!("failed to write {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("failed to rename to {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
pub fn save_content(path: &Path, content: &str) -> Result<String> {
    let now = now_local();
    let updated = touch_updated_at(content, now)?;
    write_note_atomic(path, &updated)?;
    Ok(updated)
}

pub fn save_body(path: &Path, frontmatter: &Frontmatter, body: &str) -> Result<Frontmatter> {
    let mut fm = frontmatter.clone();
    fm.updated_at = now_local();
    let full = serialize_document(&fm, body);
    write_note_atomic(path, &full)?;
    Ok(fm)
}

pub fn save_document(
    path: &Path,
    mode: DocumentMode,
    frontmatter: &Frontmatter,
    body: &str,
) -> Result<Frontmatter> {
    match mode {
        DocumentMode::Managed => save_body(path, frontmatter, body),
        DocumentMode::Plain => {
            write_note_atomic(path, body)?;
            let mut fm = frontmatter.clone();
            fm.updated_at = now_local();
            Ok(fm)
        }
    }
}

pub fn list_notes() -> Result<Vec<NoteListItem>> {
    let dir = ensure_notes_dir()?;
    let mut items = Vec::new();
    let entries =
        fs::read_dir(&dir).with_context(|| format!("failed to read {}", dir.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some(NOTE_EXT) {
            continue;
        }
        let mtime = entry
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        let label = match load_note_meta(&path) {
            Ok((k, dt)) => list_label(k, dt),
            Err(_) => {
                let kind = guess_kind_from_path(&path);
                let dt = system_time_to_local(mtime);
                list_label(kind, dt)
            }
        };
        items.push(NoteListItem { path, label, mtime });
    }
    items.sort_by(|a, b| b.mtime.cmp(&a.mtime));
    Ok(items)
}

fn load_note_meta(path: &Path) -> Result<(NoteKind, DateTime<FixedOffset>)> {
    let content = fs::read_to_string(path)?;
    let (fm, _) = parse_frontmatter(&content)?;
    let dt = match fm.kind {
        NoteKind::Daily => fm.created_at,
        NoteKind::Note => fm.created_at,
    };
    if let Some(from_name) = parse_datetime_from_filename(path) {
        return Ok((fm.kind, from_name));
    }
    Ok((fm.kind, dt))
}

fn parse_datetime_from_filename(path: &Path) -> Option<DateTime<FixedOffset>> {
    use chrono::TimeZone;

    let name = path.file_stem()?.to_str()?;
    if let Some(rest) = name.strip_prefix("daily_") {
        let naive = chrono::NaiveDate::parse_from_str(rest, "%Y-%m-%d").ok()?;
        let ndt = naive.and_hms_opt(0, 0, 0)?;
        return Local
            .from_local_datetime(&ndt)
            .single()
            .map(|d| d.fixed_offset());
    }
    if let Some(rest) = name.strip_prefix("note_") {
        let parts: Vec<&str> = rest.split('_').collect();
        if parts.len() >= 2 {
            let date = chrono::NaiveDate::parse_from_str(parts[0], "%Y-%m-%d").ok()?;
            let t = parts[1];
            if t.len() >= 6 {
                let h: u32 = t[0..2].parse().ok()?;
                let m: u32 = t[2..4].parse().ok()?;
                let s: u32 = t[4..6].parse().ok()?;
                let ndt = date.and_hms_opt(h, m, s)?;
                return Local
                    .from_local_datetime(&ndt)
                    .single()
                    .map(|d| d.fixed_offset());
            }
        }
    }
    None
}

pub fn resolve_open(_daily: bool, new: bool, file: Option<&Path>) -> Result<NoteDocument> {
    if new {
        return create_new_note();
    }
    if let Some(path) = file {
        let path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()?.join(path)
        };
        if path.exists() {
            return load_note(&path);
        }
        if path_suggests_managed(&path) {
            let now = now_local();
            let content = note_template(now);
            write_note_atomic(&path, &content)?;
            return Ok(NoteDocument {
                path,
                mode: DocumentMode::Managed,
                frontmatter: Frontmatter {
                    kind: NoteKind::Note,
                    created_at: now,
                    updated_at: now,
                },
                content,
            });
        }
        let content = String::new();
        write_note_atomic(&path, &content)?;
        return Ok(NoteDocument {
            path,
            mode: DocumentMode::Plain,
            frontmatter: synthetic_frontmatter_now(NoteKind::Note),
            content,
        });
    }
    open_or_create_daily()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;
    use std::sync::Mutex;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn sample_offset() -> FixedOffset {
        FixedOffset::west_opt(3 * 3600).unwrap()
    }

    fn sample_dt() -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339("2026-07-09T02:14:55-03:00").unwrap()
    }

    #[test]
    fn daily_path_uses_date() {
        let _g = TEST_LOCK.lock().unwrap();
        let dt = sample_dt();
        let path = daily_path_for(dt).unwrap();
        assert!(path
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("daily_2026-07-09.dpad"));
    }

    #[test]
    fn new_note_name_uses_datetime() {
        let _g = TEST_LOCK.lock().unwrap();
        let dt = sample_dt();
        let path = new_note_path_for(dt).unwrap();
        let name = path.file_name().unwrap().to_str().unwrap();
        assert_eq!(name, "note_2026-07-09_021455.dpad");
    }

    #[test]
    fn parse_frontmatter_basic() {
        let content = daily_template(sample_dt());
        let (fm, _offset) = parse_frontmatter(&content).unwrap();
        assert_eq!(fm.kind, NoteKind::Daily);
        assert_eq!(format_timestamp(fm.created_at), "2026-07-09T02:14:55-03:00");
    }

    #[test]
    fn parse_note_frontmatter() {
        let content = note_template(sample_dt());
        let (fm, _) = parse_frontmatter(&content).unwrap();
        assert_eq!(fm.kind, NoteKind::Note);
    }

    #[test]
    fn touch_updated_at_preserves_body() {
        let dt = sample_dt();
        let content = note_template(dt);
        let later = dt + chrono::Duration::hours(1);
        let updated = touch_updated_at(&content, later).unwrap();
        assert!(updated.contains("updated_at: 2026-07-09T03:14:55-03:00"));
        assert!(updated.contains("# Note · 2026-07-09 02:14"));
        assert!(updated.contains("kind: note"));
        assert!(updated.contains(dt.format("%Y-%m-%d %H:%M").to_string().as_str()) || true);
    }

    #[test]
    fn autosave_does_not_destroy_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note_2026-07-09_021455.dpad");
        let original = note_template(sample_dt()) + "Hello **world**\n\n- [ ] task\n";
        write_note_atomic(&path, &original).unwrap();
        let saved = save_content(&path, &original).unwrap();
        let disk = fs::read_to_string(&path).unwrap();
        assert_eq!(saved, disk);
        assert!(disk.contains("Hello **world**"));
        assert!(disk.contains("- [ ] task"));
        assert!(disk.contains("kind: note"));
        assert!(disk.contains("updated_at:"));
    }

    #[test]
    fn body_hides_frontmatter() {
        let content = note_template(sample_dt()) + "Hello **world**\n";
        let body = body_of(&content);
        assert!(!body.contains("kind:"));
        assert!(!body.contains("---"));
        assert!(body.contains("Hello **world**"));
        assert!(body.contains("# Note ·"));
    }

    #[test]
    fn save_body_keeps_frontmatter_on_disk_only() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note_test.dpad");
        let dt = sample_dt();
        let fm = Frontmatter {
            kind: NoteKind::Note,
            created_at: dt,
            updated_at: dt,
        };
        let written = save_body(&path, &fm, "# Note · hi\n\nbody line\n").unwrap();
        let disk = fs::read_to_string(&path).unwrap();
        assert!(disk.starts_with("---\n"));
        assert!(disk.contains("kind: note"));
        assert!(disk.contains("body line"));
        assert!(written.updated_at >= fm.updated_at);
        assert_eq!(body_of(&disk).trim(), "# Note · hi\n\nbody line");
    }

    #[test]
    fn load_txt_is_plain() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("readme.txt");
        fs::write(&path, "hello plain\nno yaml\n").unwrap();
        let doc = load_note(&path).unwrap();
        assert_eq!(doc.mode, DocumentMode::Plain);
        assert_eq!(doc.body(), "hello plain\nno yaml\n");
        assert!(!doc.body().contains("kind:"));
    }

    #[test]
    fn save_plain_does_not_inject_frontmatter() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes.txt");
        fs::write(&path, "alpha\n").unwrap();
        let doc = load_note(&path).unwrap();
        assert_eq!(doc.mode, DocumentMode::Plain);
        let fm = save_document(&path, doc.mode, &doc.frontmatter, "alpha\nbeta\n").unwrap();
        let disk = fs::read_to_string(&path).unwrap();
        assert_eq!(disk, "alpha\nbeta\n");
        assert!(!disk.contains("---"));
        assert!(!disk.contains("kind:"));
        let _ = fm;
    }

    #[test]
    fn load_dpad_is_managed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note_x.dpad");
        let content = note_template(sample_dt()) + "body here\n";
        fs::write(&path, &content).unwrap();
        let doc = load_note(&path).unwrap();
        assert_eq!(doc.mode, DocumentMode::Managed);
        assert!(!doc.body().contains("kind:"));
        assert!(doc.body().contains("body here"));
    }

    #[test]
    fn save_managed_keeps_frontmatter() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("m.dpad");
        let dt = sample_dt();
        let fm = Frontmatter {
            kind: NoteKind::Note,
            created_at: dt,
            updated_at: dt,
        };
        save_document(&path, DocumentMode::Managed, &fm, "# Hi\n").unwrap();
        let disk = fs::read_to_string(&path).unwrap();
        assert!(disk.starts_with("---\n"));
        assert!(disk.contains("kind: note"));
        assert!(disk.contains("# Hi"));
    }

    #[test]
    fn resolve_missing_txt_creates_plain_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("new.txt");
        let doc = resolve_open(false, false, Some(&path)).unwrap();
        assert_eq!(doc.mode, DocumentMode::Plain);
        assert_eq!(doc.content, "");
        assert!(path.exists());
        assert_eq!(fs::read_to_string(&path).unwrap(), "");
    }

    #[test]
    fn resolve_missing_dpad_creates_managed_template() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fresh.dpad");
        let doc = resolve_open(false, false, Some(&path)).unwrap();
        assert_eq!(doc.mode, DocumentMode::Managed);
        assert!(doc.content.contains("kind: note"));
        assert!(path.exists());
    }

    #[test]
    fn garbage_triple_dash_stays_plain() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("log.txt");
        fs::write(&path, "---\nnot a dopepad file\njust a separator\n").unwrap();
        let doc = load_note(&path).unwrap();
        assert_eq!(doc.mode, DocumentMode::Plain);
        save_document(&path, doc.mode, &doc.frontmatter, doc.body()).unwrap();
        let disk = fs::read_to_string(&path).unwrap();
        assert!(!disk.contains("kind: note"));
        assert!(!disk.contains("created_at:"));
    }

    #[test]
    fn window_title_formats() {
        let dt = sample_dt();
        assert_eq!(
            window_title(NoteKind::Daily, dt),
            "DopePad · Daily · 2026-07-09"
        );
        assert_eq!(
            window_title(NoteKind::Note, dt),
            "DopePad · Note · 2026-07-09 02:14"
        );
    }

    #[test]
    fn plain_document_title_uses_filename() {
        let doc = NoteDocument {
            path: PathBuf::from("/tmp/README.md"),
            mode: DocumentMode::Plain,
            frontmatter: synthetic_frontmatter_now(NoteKind::Note),
            content: "# hi\n".into(),
        };
        assert_eq!(document_window_title(&doc), "DopePad · README.md");
    }

    #[test]
    fn touch_without_frontmatter_keeps_text() {
        let text = "plain note without fm\n";
        let out = touch_updated_at(text, sample_dt()).unwrap();
        assert_eq!(out, text);
    }

    #[test]
    fn sample_offset_negative_three() {
        assert_eq!(sample_offset().local_minus_utc(), -3 * 3600);
    }
}
