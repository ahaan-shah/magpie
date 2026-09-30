//! Magpie's engine: data model, persistence, analytics and import/export.
//! Nothing in here knows about the UI.

pub mod analytics;
pub mod budget;
pub mod db;
pub mod demo;
pub mod fx;
pub mod goals;
pub mod io;
pub mod model;
pub mod money;
pub mod quick;
pub mod receipts;
pub mod recurring;
pub mod store;

pub use model::*;
pub use money::Cur;
pub use store::Store;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("csv: {0}")]
    Csv(#[from] csv::Error),
    #[error("xlsx: {0}")]
    Xlsx(#[from] rust_xlsxwriter::XlsxError),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("network: {0}")]
    Http(String),
    #[error("{0}")]
    Msg(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Where Magpie keeps its database and receipts:
/// `~/.local/share/magpie` on Linux, `~/Library/Application Support/magpie`
/// on macOS. `MAGPIE_DATA_DIR` overrides it (handy for demos and tests).
pub fn data_dir() -> std::path::PathBuf {
    if let Some(p) = std::env::var_os("MAGPIE_DATA_DIR") {
        return p.into();
    }
    directories::ProjectDirs::from("dev", "magpie", "magpie")
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from(".magpie"))
}

/// `~/Downloads`, falling back to the home directory.
pub fn downloads_dir() -> std::path::PathBuf {
    directories::UserDirs::new()
        .and_then(|u| {
            u.download_dir()
                .map(|p| p.to_path_buf())
                .or_else(|| Some(u.home_dir().to_path_buf()))
        })
        .unwrap_or_else(|| std::path::PathBuf::from("."))
}
