//! Screen positions of a few named UI elements, recorded only while the
//! demo film is rendering (so the scripted cursor and camera can aim at real
//! buttons). Disabled, `record` is a single relaxed atomic load.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

static ON: AtomicBool = AtomicBool::new(false);
static MAP: Mutex<Option<HashMap<String, egui::Rect>>> = Mutex::new(None);

#[allow(dead_code)]
pub fn enable() {
    ON.store(true, Ordering::Relaxed);
}

#[inline]
pub fn record(key: impl FnOnce() -> String, rect: egui::Rect) {
    if ON.load(Ordering::Relaxed)
        && let Ok(mut m) = MAP.lock()
    {
        m.get_or_insert_with(HashMap::new).insert(key(), rect);
    }
}

#[allow(dead_code)]
pub fn get(key: &str) -> Option<egui::Rect> {
    MAP.lock().ok()?.as_ref()?.get(key).copied()
}
