mod filesystem;

pub use filesystem::{
    EolStyle, FileData, IoError, load_document, load_sidecar, save_document, save_sidecar,
    sidecar_path_for,
};
