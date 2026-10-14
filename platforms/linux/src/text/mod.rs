//! Text measurement with Pango, on the JS thread, and the layouts the `Text`
//! component draws on the main thread with the very same configuration.
//!
//! The core sends the exact `AttributedText` JSON it measured, so drawing
//! looks the parse up under that string and lays it out again at the frame
//! width the core settled on: the same lines come out on both threads.

mod layout;
mod parse;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

use native_shell_core::host::{TextMetrics, WidthMode};

pub use layout::{build, TextLayout};
pub use parse::{parse, Align, ParsedText};

/// Device pixels per point and the user's text size factor, as `f32` bits.
/// Written on the main thread before the environment reaches the core, read
/// by the measurer on the JS thread.
static DEVICE_SCALE: AtomicU32 = AtomicU32::new(0x3f80_0000);
static FONT_SCALE: AtomicU32 = AtomicU32::new(0x3f80_0000);

pub fn font_scale() -> f32 {
    f32::from_bits(FONT_SCALE.load(Ordering::Relaxed))
}

pub fn device_scale() -> f32 {
    f32::from_bits(DEVICE_SCALE.load(Ordering::Relaxed))
}

/// Records the display's scale and the user's font scale (text scales fully in
/// the renderer: see docs/architecture.md, "Text"). Call it before handing
/// the same values to the core in its `Environment`; returns whether anything
/// changed.
pub fn set_scales(device: f32, font: f32) -> bool {
    let valid = |v: f32| if v.is_finite() && v > 0.0 { v } else { 1.0 };
    let device = valid(device).to_bits();
    let font = valid(font).to_bits();
    let device_changed = DEVICE_SCALE.swap(device, Ordering::Relaxed) != device;
    let font_changed = FONT_SCALE.swap(font, Ordering::Relaxed) != font;
    device_changed || font_changed
}

/// JSON, width mode, width, font scale and device scale bits.
type MetricsKey = (String, u8, u32, u32, u32);

fn metrics_cache() -> &'static Mutex<HashMap<MetricsKey, TextMetrics>> {
    static CACHE: OnceLock<Mutex<HashMap<MetricsKey, TextMetrics>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

const METRICS_CACHE_LIMIT: usize = 5_000;

/// `measure_text` (docs/protocol-json.md, "Measurement callbacks").
pub fn measure(attributed_json: &str, width: WidthMode) -> TextMetrics {
    measure_at(attributed_json, width, font_scale(), device_scale())
}

pub fn measure_at(attributed_json: &str, width: WidthMode, font_scale: f32, device_scale: f32) -> TextMetrics {
    let (mode, w) = match width {
        WidthMode::AtMost(w) if w.is_finite() && w >= 0.0 => (1, w),
        WidthMode::Exactly(w) if w.is_finite() && w >= 0.0 => (2, w),
        _ => (0, f32::INFINITY),
    };
    let key = (attributed_json.to_owned(), mode, w.to_bits(), font_scale.to_bits(), device_scale.to_bits());
    if let Some(hit) = metrics_cache().lock().ok().and_then(|c| c.get(&key).copied()) {
        return hit;
    }
    let text = parse(attributed_json);
    let laid = build(&text, (mode != 0).then_some(w), font_scale);
    let width = match mode {
        0 => laid.width,
        1 => laid.width.min(w),
        _ => w,
    };
    // Widths stay exact (the core snaps them to pixels and measures again at
    // the snapped width); heights round up to device pixels.
    let s = device_scale;
    let height = ((laid.height * s) - 0.001).ceil().max(0.0) / s;
    let metrics = TextMetrics { width, height, baseline: laid.baseline };
    if let Ok(mut cache) = metrics_cache().lock() {
        if cache.len() >= METRICS_CACHE_LIMIT {
            cache.clear();
        }
        cache.insert(key, metrics);
    }
    metrics
}

#[cfg(test)]
mod tests;
