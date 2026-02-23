use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub type ColorMap = BTreeMap<usize, u8>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EolStyle {
    Lf,
    Crlf,
}

#[derive(Debug, Clone)]
pub struct FileData {
    pub text: String,
    pub eol: EolStyle,
}

#[derive(Debug, Error)]
pub enum IoError {
    #[error("failed reading file {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed writing file {path}: {source}")]
    Write {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed parsing sidecar {path}: {source}")]
    SidecarParse {
        path: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("failed serializing sidecar {path}: {source}")]
    SidecarSerialize {
        path: String,
        #[source]
        source: serde_json::Error,
    },
}

#[derive(Debug, Serialize, Deserialize)]
struct Sidecar {
    #[serde(default)]
    char_colors: ColorMap,
    #[serde(default)]
    line_colors: ColorMap,
}

pub fn load_document(path: &Path) -> Result<FileData, IoError> {
    let bytes = fs::read(path).map_err(|source| IoError::Read {
        path: path.display().to_string(),
        source,
    })?;
    let raw = String::from_utf8_lossy(&bytes).to_string();
    let eol = detect_eol(&raw);
    let text = raw.replace("\r\n", "\n");
    Ok(FileData { text, eol })
}

pub fn save_document(path: &Path, text: &str, eol: EolStyle) -> Result<(), IoError> {
    let out = match eol {
        EolStyle::Lf => text.to_string(),
        EolStyle::Crlf => text.replace('\n', "\r\n"),
    };
    fs::write(path, out).map_err(|source| IoError::Write {
        path: path.display().to_string(),
        source,
    })
}

pub fn detect_eol(content: &str) -> EolStyle {
    if content.contains("\r\n") {
        EolStyle::Crlf
    } else {
        EolStyle::Lf
    }
}

pub fn sidecar_path_for(path: &Path) -> PathBuf {
    let mut out = PathBuf::from(path);
    let mut file_name: OsString = path
        .file_name()
        .map(|s| s.to_os_string())
        .unwrap_or_else(|| OsString::from("untitled.txt"));
    file_name.push(".dopedpad.json");
    out.set_file_name(file_name);
    out
}

pub fn load_sidecar(path: &Path) -> Result<ColorMap, IoError> {
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let raw = fs::read_to_string(path).map_err(|source| IoError::Read {
        path: path.display().to_string(),
        source,
    })?;
    let parsed: Sidecar = serde_json::from_str(&raw).map_err(|source| IoError::SidecarParse {
        path: path.display().to_string(),
        source,
    })?;
    if parsed.char_colors.is_empty() {
        Ok(parsed.line_colors)
    } else {
        Ok(parsed.char_colors)
    }
}

pub fn save_sidecar(path: &Path, colors: &ColorMap) -> Result<(), IoError> {
    let sidecar = Sidecar {
        char_colors: colors.clone(),
        line_colors: BTreeMap::new(),
    };
    let raw =
        serde_json::to_string_pretty(&sidecar).map_err(|source| IoError::SidecarSerialize {
            path: path.display().to_string(),
            source,
        })?;
    fs::write(path, raw).map_err(|source| IoError::Write {
        path: path.display().to_string(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use tempfile::tempdir;

    use super::{
        EolStyle, detect_eol, load_document, load_sidecar, save_document, save_sidecar,
        sidecar_path_for,
    };

    #[test]
    fn eol_detection_and_preservation_work() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("f.txt");
        std::fs::write(&path, "a\r\nb\r\n").expect("write");
        let doc = load_document(&path).expect("load");
        assert_eq!(doc.eol, EolStyle::Crlf);
        assert_eq!(doc.text, "a\nb\n");

        save_document(&path, &doc.text, doc.eol).expect("save");
        let saved = std::fs::read_to_string(&path).expect("read");
        assert!(saved.contains("\r\n"));
        assert_eq!(detect_eol(&saved), EolStyle::Crlf);
    }

    #[test]
    fn sidecar_roundtrip_works() {
        let dir = tempdir().expect("tempdir");
        let txt = dir.path().join("note.txt");
        let sidecar = sidecar_path_for(&txt);
        let mut map = BTreeMap::new();
        map.insert(0, 2);
        map.insert(10, 8);

        save_sidecar(&sidecar, &map).expect("save sidecar");
        let loaded = load_sidecar(&sidecar).expect("load sidecar");
        assert_eq!(loaded, map);
    }
}
