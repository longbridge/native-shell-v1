//! `Image`: `http(s)` URLs, absolute file paths (`file://` too) and `data:`
//! URIs. Bytes are fetched and decoded on GLib's blocking pool, downsampled to
//! the pixels the node can show, and cached decoded on the main thread.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use gtk::prelude::*;
use gtk::{gdk, gdk_pixbuf, gio, glib, graphene, gsk};

use native_shell_core::protocol::{Frame, Prop, PropValue};

use crate::component::{Component, NodeCx};
use crate::mount::color;
use crate::node_view::{clamp_radii, rgba, rounded_rect, NodeView};

/// Network bodies above this are refused.
const MAX_BYTES: u64 = 32 << 20;
/// Decoded bytes kept in the cache.
const CACHE_BYTES: usize = 64 << 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContentMode {
    #[default]
    Cover,
    Contain,
    Fill,
    Center,
}

impl ContentMode {
    pub fn from_raw(v: i32) -> ContentMode {
        match v {
            1 => ContentMode::Contain,
            2 => ContentMode::Fill,
            3 => ContentMode::Center,
            _ => ContentMode::Cover,
        }
    }
}

/// Where an image of `natural` points goes in a `w`×`h` box: `(x, y, w, h)`.
pub fn place(mode: ContentMode, natural: (f32, f32), w: f32, h: f32) -> (f32, f32, f32, f32) {
    let (iw, ih) = natural;
    if iw <= 0.0 || ih <= 0.0 {
        return (0.0, 0.0, w, h);
    }
    let (dw, dh) = match mode {
        ContentMode::Fill => return (0.0, 0.0, w, h),
        ContentMode::Center => (iw, ih),
        ContentMode::Cover => {
            let s = (w / iw).max(h / ih);
            (iw * s, ih * s)
        }
        ContentMode::Contain => {
            let s = (w / iw).min(h / ih);
            (iw * s, ih * s)
        }
    };
    ((w - dw) / 2.0, (h - dh) / 2.0, dw, dh)
}

/// A decoded image and the device scale it was decoded for.
#[derive(Clone)]
struct Decoded {
    texture: gdk::Texture,
    scale: f32,
}

#[derive(Default)]
struct State {
    source: RefCell<Option<String>>,
    image: RefCell<Option<Decoded>>,
    mode: Cell<ContentMode>,
    tint: Cell<Option<u32>>,
    /// Bumped per load so a stale result never lands.
    token: Cell<u64>,
    loading: Cell<bool>,
}

pub struct Image {
    state: Rc<State>,
}

impl Image {
    pub fn new(cx: &NodeCx) -> Image {
        let state = Rc::new(State::default());
        let s = state.clone();
        cx.view.set_draw(Some(Box::new(move |view, snapshot| {
            draw(&s, view, snapshot)
        })));
        Image { state }
    }

    fn load(&self, view: &NodeView) {
        let s = &self.state;
        let Some(source) = s.source.borrow().clone() else {
            return;
        };
        let (w, h) = view.frame().map_or((0.0, 0.0), |f| (f.width, f.height));
        if w <= 0.0 || h <= 0.0 || s.loading.get() || s.image.borrow().is_some() {
            return;
        }
        let scale = view.device_scale();
        let max_px = ((w.max(h) * scale).ceil() as u32).max(1);
        let key = (source.clone(), max_px);
        if let Some(hit) = cache_get(&key) {
            s.image.replace(Some(hit));
            view.queue_draw();
            return;
        }
        let token = s.token.get() + 1;
        s.token.set(token);
        s.loading.set(true);
        let (state, weak) = (Rc::downgrade(s), view.downgrade());
        glib::spawn_future_local(async move {
            let result =
                gio::spawn_blocking(move || read(&source).and_then(|bytes| decode(&bytes, max_px)))
                    .await;
            let Some(state) = state.upgrade() else { return };
            if state.token.get() != token {
                return;
            }
            state.loading.set(false);
            match result {
                Ok(Ok(texture)) => {
                    let decoded = Decoded { texture, scale };
                    cache_put(key, decoded.clone());
                    state.image.replace(Some(decoded));
                    if let Some(view) = weak.upgrade() {
                        view.queue_draw();
                    }
                }
                Ok(Err(e)) => eprintln!("[native-shell] image `{}`: {e}", short(&key.0)),
                Err(_) => {}
            }
        });
    }
}

impl Component for Image {
    fn set_prop(&mut self, cx: &NodeCx, prop: Prop, value: &PropValue) -> bool {
        let s = &self.state;
        match prop {
            Prop::ImageSource => {
                let source = match value {
                    PropValue::Str(v) if !v.is_empty() => Some(v.clone()),
                    _ => None,
                };
                if *s.source.borrow() == source {
                    return true;
                }
                s.source.replace(source);
                s.image.replace(None);
                s.token.set(s.token.get() + 1);
                s.loading.set(false);
                cx.view.queue_draw();
                self.load(cx.view);
            }
            Prop::ContentMode => {
                s.mode
                    .set(ContentMode::from_raw(if let PropValue::I32(v) = value {
                        *v
                    } else {
                        0
                    }));
                cx.view.queue_draw();
            }
            Prop::TintColor => {
                s.tint.set(color(value));
                cx.view.queue_draw();
            }
            _ => return false,
        }
        true
    }

    fn set_frame(&mut self, cx: &NodeCx, _frame: Frame) {
        self.load(cx.view);
    }

    fn unmount(&mut self, _cx: &NodeCx) {
        self.state.token.set(self.state.token.get() + 1);
    }
}

fn draw(s: &State, view: &NodeView, snapshot: &gtk::Snapshot) {
    let image = s.image.borrow();
    let Some(Decoded { texture, scale }) = image.as_ref() else {
        return;
    };
    let (w, h) = view.paint_size();
    let natural = (
        texture.width() as f32 / scale,
        texture.height() as f32 / scale,
    );
    let (x, y, dw, dh) = place(s.mode.get(), natural, w, h);
    let rect = graphene::Rect::new(x, y, dw, dh);
    // Images clip to the node's rounded shape, like CSS `img` with border-radius.
    let bounds = graphene::Rect::new(0.0, 0.0, w, h);
    snapshot.push_rounded_clip(&rounded_rect(bounds, clamp_radii(view.style().radii, w, h)));
    match s.tint.get() {
        // Template rendering: the image's alpha masks the tint color.
        Some(tint) => {
            snapshot.push_mask(gsk::MaskMode::Alpha);
            snapshot.append_scaled_texture(texture, gsk::ScalingFilter::Trilinear, &rect);
            snapshot.pop();
            snapshot.append_color(&rgba(tint), &rect);
            snapshot.pop();
        }
        None => snapshot.append_scaled_texture(texture, gsk::ScalingFilter::Trilinear, &rect),
    }
    snapshot.pop();
}

fn short(source: &str) -> String {
    source.chars().take(80).collect()
}

/// The bytes behind a source. Runs off the main thread.
fn read(source: &str) -> Result<Vec<u8>, String> {
    if source.starts_with("http://") || source.starts_with("https://") {
        let response = agent().get(source).call().map_err(|e| e.to_string())?;
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(
            &mut std::io::Read::take(response.into_reader(), MAX_BYTES),
            &mut bytes,
        )
        .map_err(|e| e.to_string())?;
        return Ok(bytes);
    }
    if let Some(rest) = source.strip_prefix("data:") {
        let (meta, data) = rest.split_once(',').ok_or("malformed data URI")?;
        if !meta.ends_with(";base64") {
            return Err("only base64 data URIs are supported".into());
        }
        return Ok(glib::base64_decode(data.trim()));
    }
    let path = if source.starts_with("file://") {
        gio::File::for_uri(source)
            .path()
            .ok_or("not a local file URI")?
    } else {
        std::path::PathBuf::from(source)
    };
    // Program-relative paths are resolved by the core; never against the cwd.
    if !path.is_absolute() {
        return Err("not an absolute path or URL".into());
    }
    std::fs::read(&path).map_err(|e| e.to_string())
}

fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        let roots = rustls::RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
        };
        let builder = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(15))
            .timeout(Duration::from_secs(30));
        // The ring provider explicitly, as the services crate does, so the
        // process-wide rustls default never matters.
        match rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        {
            Ok(b) => builder
                .tls_config(Arc::new(
                    b.with_root_certificates(roots).with_no_client_auth(),
                ))
                .build(),
            Err(_) => builder.build(),
        }
    })
}

/// Decodes at most `max_px` pixels on the longer side (EXIF orientation
/// applied). Runs off the main thread.
pub fn decode(bytes: &[u8], max_px: u32) -> Result<gdk::Texture, String> {
    let loader = gdk_pixbuf::PixbufLoader::new();
    loader.connect_size_prepared(move |loader, w, h| {
        let longer = w.max(h);
        if longer > max_px as i32 && longer > 0 {
            let f = max_px as f64 / longer as f64;
            loader.set_size(
                ((w as f64 * f).round() as i32).max(1),
                ((h as f64 * f).round() as i32).max(1),
            );
        }
    });
    let written = loader.write(bytes);
    let closed = loader.close();
    written.and(closed).map_err(|e| e.to_string())?;
    let pixbuf = loader.pixbuf().ok_or("no image")?;
    let pixbuf = pixbuf.apply_embedded_orientation().unwrap_or(pixbuf);
    let format = if pixbuf.has_alpha() {
        gdk::MemoryFormat::R8g8b8a8
    } else {
        gdk::MemoryFormat::R8g8b8
    };
    let texture = gdk::MemoryTexture::new(
        pixbuf.width(),
        pixbuf.height(),
        format,
        &pixbuf.read_pixel_bytes(),
        pixbuf.rowstride() as usize,
    );
    Ok(texture.upcast())
}

type Key = (String, u32);

thread_local! {
    static CACHE: RefCell<(HashMap<Key, Decoded>, VecDeque<Key>, usize)> = RefCell::default();
}

fn cost(d: &Decoded) -> usize {
    d.texture.width().max(0) as usize * d.texture.height().max(0) as usize * 4
}

fn cache_get(key: &Key) -> Option<Decoded> {
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        let hit = c.0.get(key).cloned()?;
        // Most recently used goes last.
        c.1.retain(|k| k != key);
        c.1.push_back(key.clone());
        Some(hit)
    })
}

fn cache_put(key: Key, value: Decoded) {
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        let size = cost(&value);
        if size > CACHE_BYTES / 4 {
            return;
        }
        if let Some(old) = c.0.insert(key.clone(), value) {
            c.2 -= cost(&old);
            c.1.retain(|k| *k != key);
        }
        c.1.push_back(key);
        c.2 += size;
        while c.2 > CACHE_BYTES {
            let Some(oldest) = c.1.pop_front() else { break };
            if let Some(gone) = c.0.remove(&oldest) {
                c.2 -= cost(&gone);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_modes_place_like_uikit() {
        // A 200×100 image in a 100×100 box.
        assert_eq!(
            place(ContentMode::Cover, (200.0, 100.0), 100.0, 100.0),
            (-50.0, 0.0, 200.0, 100.0)
        );
        assert_eq!(
            place(ContentMode::Contain, (200.0, 100.0), 100.0, 100.0),
            (0.0, 25.0, 100.0, 50.0)
        );
        assert_eq!(
            place(ContentMode::Fill, (200.0, 100.0), 100.0, 100.0),
            (0.0, 0.0, 100.0, 100.0)
        );
        assert_eq!(
            place(ContentMode::Center, (20.0, 10.0), 100.0, 100.0),
            (40.0, 45.0, 20.0, 10.0)
        );
        assert_eq!(ContentMode::from_raw(7), ContentMode::Cover);
    }

    #[test]
    fn sources_resolve_safely() {
        assert!(read("relative/icon.png").is_err());
        assert!(read("data:image/png,raw").is_err());
        assert_eq!(read("data:text/plain;base64,aGk=").unwrap(), b"hi");
        assert!(read("/nonexistent/native-shell/image.png").is_err());
    }
}
