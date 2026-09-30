//! Receipt images are decoded on a background thread, downscaled, and kept
//! in a small LRU so browsing receipts never balloons memory.

use egui::{ColorImage, TextureHandle, TextureOptions};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::mpsc;

const CAPACITY: usize = 8;
const MAX_SIDE: u32 = 1400;

#[derive(Default)]
pub struct ReceiptCache {
    textures: HashMap<String, TextureHandle>,
    order: VecDeque<String>,
    loading: HashMap<String, mpsc::Receiver<Option<ColorImage>>>,
    failed: Vec<String>,
}

impl ReceiptCache {
    /// Returns the texture if ready; otherwise starts loading it.
    pub fn get(&mut self, ctx: &egui::Context, key: &str, path: PathBuf) -> Option<TextureHandle> {
        if let Some(t) = self.textures.get(key) {
            let t = t.clone();
            self.order.retain(|k| k != key);
            self.order.push_back(key.to_string());
            return Some(t);
        }
        if self.failed.iter().any(|k| k == key) {
            return None;
        }
        if let Some(rx) = self.loading.get(key) {
            match rx.try_recv() {
                Ok(Some(img)) => {
                    self.loading.remove(key);
                    let tex = ctx.load_texture(format!("receipt-{key}"), img, TextureOptions::LINEAR);
                    self.textures.insert(key.to_string(), tex.clone());
                    self.order.push_back(key.to_string());
                    while self.order.len() > CAPACITY {
                        if let Some(old) = self.order.pop_front() {
                            self.textures.remove(&old);
                        }
                    }
                    return Some(tex);
                }
                Ok(None) | Err(mpsc::TryRecvError::Disconnected) => {
                    self.loading.remove(key);
                    self.failed.push(key.to_string());
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
            return None;
        }
        let (tx, rx) = mpsc::channel();
        let ctx2 = ctx.clone();
        std::thread::spawn(move || {
            let img = image::open(&path).ok().map(|i| {
                let i = if i.width().max(i.height()) > MAX_SIDE {
                    i.thumbnail(MAX_SIDE, MAX_SIDE)
                } else {
                    i
                };
                let rgba = i.to_rgba8();
                ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw())
            });
            let _ = tx.send(img);
            ctx2.request_repaint();
        });
        self.loading.insert(key.to_string(), rx);
        None
    }
}
