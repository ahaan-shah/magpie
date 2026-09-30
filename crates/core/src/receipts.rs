//! Receipt files are stored content-addressed in `<data>/receipts/<blake3>.<ext>`
//! so attaching the same photo twice costs no extra disk.

use crate::model::*;
use crate::store::Store;
use crate::{Error, Result};
use std::path::{Path, PathBuf};

pub const ALLOWED: &[&str] = &["png", "jpg", "jpeg", "webp", "pdf"];

fn dir(store: &Store) -> Result<PathBuf> {
    let base = store
        .dir()
        .ok_or_else(|| Error::Msg("receipts need an on-disk database".into()))?;
    let d = base.join("receipts");
    std::fs::create_dir_all(&d)?;
    Ok(d)
}

pub fn path_of(store: &Store, r: &Receipt) -> Option<PathBuf> {
    Some(
        store
            .dir()?
            .join("receipts")
            .join(format!("{}.{}", r.hash, r.ext)),
    )
}

pub fn is_image(r: &Receipt) -> bool {
    matches!(r.ext.as_str(), "png" | "jpg" | "jpeg" | "webp")
}

/// Copies `src` into the receipts folder and links it to `txn`.
pub fn attach(store: &mut Store, txn: Id, src: &Path) -> Result<Id> {
    let ext = src
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .filter(|e| ALLOWED.contains(&e.as_str()))
        .ok_or_else(|| Error::Msg("receipts must be PNG, JPEG, WebP or PDF".into()))?;
    let bytes = std::fs::read(src)?;
    let hash = blake3::hash(&bytes).to_hex().to_string();
    let dest = dir(store)?.join(format!("{hash}.{ext}"));
    if !dest.exists() {
        std::fs::write(&dest, &bytes)?;
    }
    let name = src
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("receipt")
        .to_string();
    store.push_receipt(Receipt {
        id: 0,
        txn,
        hash,
        ext,
        name,
    })
}

/// Unlinks a receipt and removes the file if nothing else references it.
pub fn remove(store: &mut Store, id: Id) -> Result<()> {
    let Some(r) = store.all_receipts().iter().find(|r| r.id == id).cloned() else {
        return Ok(());
    };
    store.delete_receipt(id)?;
    let still_used = store.all_receipts().iter().any(|x| x.hash == r.hash);
    if !still_used {
        if let Some(p) = path_of(store, &r) {
            let _ = std::fs::remove_file(p);
        }
    }
    Ok(())
}
