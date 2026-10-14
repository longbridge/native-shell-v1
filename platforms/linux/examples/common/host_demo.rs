//! What a host app registers for examples/watchlist: a quote module that
//! pushes ticks while a script listens, the signed-in user, and the app's own
//! stock cell. Prices are fictional and move in a random walk.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use adw::prelude::*;
use gtk::{glib, pango};
use native_shell_gtk::{
    CallContext, EventSink, HostComponent, HostComponentType, HostMethod, HostModule, HostModuleDescriptor, MethodKind,
    Responder, Runtime, ThemeConfig, WidthMode,
};
use serde_json::{json, Value};

pub fn register(runtime: &Runtime) {
    runtime.register_module(QuoteModule::new(runtime.clone())).expect("demo/quote");
    runtime.register_module(UserModule).expect("demo/user");
    runtime.register_component(stock_cell()).expect("demo-stock-cell");
    // An app maps its own design tokens onto the shell's; here, the accent.
    let accent = |hex: &str| BTreeMap::from([("primary".to_string(), hex.to_string())]);
    runtime.set_theme(Some(ThemeConfig { light: accent(ACCENT_LIGHT), dark: accent(ACCENT_DARK) })).expect("theme");
}

/// The example app's accent, the shell's `primary` in each appearance.
const ACCENT_LIGHT: &str = "#2563eb";
const ACCENT_DARK: &str = "#60a5fa";

fn method(name: &str, kind: MethodKind, ts: &str) -> HostMethod {
    HostMethod { name: name.into(), kind, ts: Some(ts.into()) }
}

/// Previous close and today's opening move of each fictional instrument.
const INSTRUMENTS: [(&str, f64, f64); 8] = [
    ("HALO.US", 182.40, 0.0124),
    ("MAPL.US", 64.15, -0.0062),
    ("KITE.US", 27.80, 0.0310),
    ("ORCA.US", 311.05, -0.0185),
    ("FERN.US", 18.92, 0.0040),
    ("PINE.HK", 46.30, -0.0015),
    ("TIDE.HK", 9.87, 0.0205),
    ("LUMA.US", 121.60, -0.0090),
];

#[derive(Default)]
struct Feed {
    prices: BTreeMap<String, f64>,
    subscribed: Vec<String>,
    listening: bool,
    timer: Option<glib::SourceId>,
}

/// `demo/quote`: `quote(symbol)`, `subscribe(symbols)`, `unsubscribe(symbols)`, event `tick`.
/// Ticks flow only while a script listens: the SDK reports listener counts through `__listeners`.
struct QuoteModule {
    runtime: Runtime,
    feed: Arc<Mutex<Feed>>,
}

impl QuoteModule {
    fn new(runtime: Runtime) -> QuoteModule {
        let prices = INSTRUMENTS.iter().map(|(s, close, change)| (s.to_string(), close * (1.0 + change))).collect();
        QuoteModule { runtime, feed: Arc::new(Mutex::new(Feed { prices, ..Feed::default() })) }
    }
}

fn quote(feed: &Feed, symbol: &str) -> Option<Value> {
    let (_, close, _) = INSTRUMENTS.iter().find(|(s, _, _)| *s == symbol)?;
    let last = *feed.prices.get(symbol)?;
    Some(json!({
        "symbol": symbol,
        "last": (last * 100.0).round() / 100.0,
        "prev_close": close,
        "change_percent": ((last - close) / close * 10000.0).round() / 100.0,
    }))
}

fn symbols(args: &[Value]) -> Vec<String> {
    args.first().and_then(Value::as_array).map(|a| a.iter().filter_map(|s| s.as_str().map(String::from)).collect()).unwrap_or_default()
}

/// Starts the one-second ticker while a script listens to subscribed symbols, stops it otherwise. Main thread.
fn update_ticking(feed: &Arc<Mutex<Feed>>, runtime: &Runtime) {
    let mut f = feed.lock().unwrap_or_else(|p| p.into_inner());
    let wanted = f.listening && !f.subscribed.is_empty();
    match (wanted, f.timer.is_some()) {
        (false, true) => {
            if let Some(timer) = f.timer.take() {
                timer.remove();
            }
        }
        (true, false) => {
            let (feed, runtime) = (feed.clone(), runtime.clone());
            f.timer = Some(glib::timeout_add_local(Duration::from_secs(1), move || {
                let tick = {
                    let mut f = feed.lock().unwrap_or_else(|p| p.into_inner());
                    let i = glib::random_int_range(0, f.subscribed.len().max(1) as i32) as usize;
                    let Some(symbol) = f.subscribed.get(i).cloned() else { return glib::ControlFlow::Continue };
                    if let Some(price) = f.prices.get_mut(&symbol) {
                        *price *= 1.0 + glib::random_double_range(-0.004, 0.004);
                    }
                    quote(&f, &symbol)
                };
                if let Some(q) = tick {
                    runtime.emit("demo/quote", "tick", &q);
                }
                glib::ControlFlow::Continue
            }));
        }
        _ => {}
    }
}

impl HostModule for QuoteModule {
    fn descriptor(&self) -> HostModuleDescriptor {
        HostModuleDescriptor {
            name: "demo/quote".into(),
            methods: vec![
                method("quote", MethodKind::Async, "(symbol: string) => Promise<Quote>"),
                method("subscribe", MethodKind::Async, "(symbols: string[]) => Promise<void>"),
                method("unsubscribe", MethodKind::Async, "(symbols: string[]) => Promise<void>"),
            ],
            events: vec!["tick".into()],
            requires_permission: true,
            dts: Some("interface Quote { symbol: string; last: number; prev_close: number; change_percent: number }".into()),
        }
    }

    /// `__listeners [event, count]`: start the feed on the first listener, stop it after the last.
    fn call_sync(&self, method: &str, args: &[Value], _cx: &CallContext) -> Result<Value, String> {
        if method == "__listeners" && args.first().and_then(Value::as_str) == Some("tick") {
            let count = args.get(1).and_then(Value::as_i64).unwrap_or(0);
            let (feed, runtime) = (self.feed.clone(), self.runtime.clone());
            glib::MainContext::default().invoke(move || {
                feed.lock().unwrap_or_else(|p| p.into_inner()).listening = count > 0;
                update_ticking(&feed, &runtime);
            });
        }
        Ok(Value::Null)
    }

    fn call(&self, method: &str, args: Vec<Value>, _cx: CallContext, respond: Responder) {
        let (feed, runtime, method) = (self.feed.clone(), self.runtime.clone(), method.to_string());
        // A short delay stands in for the network round trip.
        glib::MainContext::default().invoke(move || {
            glib::timeout_add_local_once(Duration::from_millis(250), move || {
                match method.as_str() {
                    "quote" => {
                        let symbol = args.first().and_then(Value::as_str).unwrap_or_default();
                        match quote(&feed.lock().unwrap_or_else(|p| p.into_inner()), symbol) {
                            Some(q) => respond.resolve(&q),
                            None => respond.reject(format!("No quote for {symbol}")),
                        }
                    }
                    "subscribe" | "unsubscribe" => {
                        {
                            let mut f = feed.lock().unwrap_or_else(|p| p.into_inner());
                            let list = symbols(&args);
                            if method == "subscribe" {
                                for s in list {
                                    if !f.subscribed.contains(&s) {
                                        f.subscribed.push(s);
                                    }
                                }
                            } else {
                                f.subscribed.retain(|s| !list.contains(s));
                            }
                        }
                        update_ticking(&feed, &runtime);
                        respond.resolve(&Value::Null);
                    }
                    other => respond.reject(format!("demo/quote has no method {other}")),
                }
            });
        });
    }
}

/// `demo/user`: `current_user()`, answered synchronously from memory.
struct UserModule;

impl HostModule for UserModule {
    fn descriptor(&self) -> HostModuleDescriptor {
        HostModuleDescriptor {
            name: "demo/user".into(),
            methods: vec![method("current_user", MethodKind::Sync, "() => { id: string; name: string } | null")],
            events: vec![],
            requires_permission: false,
            dts: None,
        }
    }

    fn call_sync(&self, _method: &str, _args: &[Value], _cx: &CallContext) -> Result<Value, String> {
        Ok(json!({ "id": "u-1", "name": "Ada Lovelace" }))
    }
}

// MARK: demo-stock-cell

const LINE_GAP: f32 = 2.0;
const VERTICAL_PADDING: f32 = 12.0;
const MIN_HEIGHT: f32 = 64.0;
const TITLE: (f64, pango::Weight) = (17.0, pango::Weight::Semibold);
const PRICE: (f64, pango::Weight) = (17.0, pango::Weight::Medium);
const DETAIL: (f64, pango::Weight) = (14.0, pango::Weight::Normal);
const CHANGE: (f64, pango::Weight) = (14.0, pango::Weight::Semibold);

/// The cell's palette: libadwaita's named colors, which follow the appearance.
const CSS: &str = "
button.demo-stock-cell { border-radius: 0; padding: 0 16px; background: none; box-shadow: none; min-height: 0; }
button.demo-stock-cell:active { background: alpha(var(--window-fg-color), 0.06); }
.demo-stock-cell .primary { color: var(--window-fg-color); }
.demo-stock-cell .muted { color: alpha(var(--window-fg-color), 0.55); }
.demo-stock-cell .gain { color: var(--success-color); }
.demo-stock-cell .loss { color: var(--error-color); }
";

fn install_css() {
    thread_local!(static INSTALLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) });
    if INSTALLED.with(|i| i.replace(true)) {
        return;
    }
    let Some(display) = gtk::gdk::Display::default() else { return };
    let provider = gtk::CssProvider::new();
    provider.load_from_string(CSS);
    gtk::style_context_add_provider_for_display(&display, &provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
}

/// Text size follows the desktop's text scaling, as the shell's own text does.
fn attributes((size, weight): (f64, pango::Weight), tabular: bool) -> pango::AttrList {
    let attrs = pango::AttrList::new();
    let scaled = size * native_shell_gtk::font_scale() as f64;
    attrs.insert(pango::AttrSize::new_size_absolute((scaled * pango::SCALE as f64) as i32));
    attrs.insert(pango::AttrInt::new_weight(weight));
    if tabular {
        attrs.insert(pango::AttrFontFeatures::new("tnum"));
    }
    attrs
}

/// The height of one line of `font`, laid out by Pango as the labels will be.
fn line_height(font: (f64, pango::Weight)) -> f32 {
    let map = pangocairo::FontMap::new();
    let context = map.create_context();
    let layout = pango::Layout::new(&context);
    layout.set_attributes(Some(&attributes(font, false)));
    layout.set_text("Ag");
    layout.pixel_size().1 as f32
}

/// `demo-stock-cell`: the app's own native quote row, placed by scripts.
///
/// Symbol and name lead; price and the day's change trail, right-aligned with
/// tabular digits so rows compare at a glance. The change carries its sign, so
/// gain and loss never depend on color alone.
struct StockCell {
    button: gtk::Button,
    symbol: gtk::Label,
    name: gtk::Label,
    last: gtk::Label,
    change: gtk::Label,
    current: Arc<Mutex<String>>,
}

fn stock_cell() -> HostComponentType {
    HostComponentType::new("demo-stock-cell", |events: EventSink| Box::new(StockCell::new(events)))
        .events(["select"])
        .props_ts("{ symbol: string; name: string; last?: number | null; change_percent?: number | null }")
        .measure(|_props, width| {
            let height = line_height(TITLE) + LINE_GAP + line_height(DETAIL) + 2.0 * VERTICAL_PADDING;
            let width = match width {
                WidthMode::AtMost(w) | WidthMode::Exactly(w) => w,
                WidthMode::Undefined => 320.0,
            };
            (width, height.ceil().max(MIN_HEIGHT))
        })
}

impl StockCell {
    fn new(events: EventSink) -> StockCell {
        install_css();
        let label = |font, tabular, class: &str, xalign: f32| {
            let l = gtk::Label::builder().xalign(xalign).css_classes([class]).build();
            l.set_attributes(Some(&attributes(font, tabular)));
            l
        };
        let symbol = label(TITLE, false, "primary", 0.0);
        let name = label(DETAIL, false, "muted", 0.0);
        name.set_ellipsize(pango::EllipsizeMode::End);
        let last = label(PRICE, true, "primary", 1.0);
        let change = label(CHANGE, true, "muted", 1.0);
        let grid = gtk::Grid::builder().column_spacing(12).row_spacing(LINE_GAP as i32).valign(gtk::Align::Center).build();
        symbol.set_hexpand(true);
        name.set_hexpand(true);
        grid.attach(&symbol, 0, 0, 1, 1);
        grid.attach(&name, 0, 1, 1, 1);
        grid.attach(&last, 1, 0, 1, 1);
        grid.attach(&change, 1, 1, 1, 1);
        let button = gtk::Button::builder().child(&grid).css_classes(["demo-stock-cell"]).build();
        let current = Arc::new(Mutex::new(String::new()));
        let selected = current.clone();
        button.connect_clicked(move |_| {
            let symbol = selected.lock().unwrap_or_else(|p| p.into_inner()).clone();
            events.emit("select", json!({ "symbol": symbol }));
        });
        StockCell { button, symbol, name, last, change, current }
    }
}

impl HostComponent for StockCell {
    fn widget(&self) -> gtk::Widget {
        self.button.clone().upcast()
    }

    fn update(&mut self, props: &Value) {
        let symbol = props["symbol"].as_str().unwrap_or_default();
        let name = props["name"].as_str().unwrap_or_default();
        let last = props["last"].as_f64();
        let change = props["change_percent"].as_f64();
        *self.current.lock().unwrap_or_else(|p| p.into_inner()) = symbol.to_string();
        self.symbol.set_text(symbol);
        self.name.set_text(name);
        self.last.set_text(&last.map_or("—".into(), |l| format!("{l:.2}")));
        self.change.set_text(&change.map_or("—".into(), |c| format!("{}{:.2}%", if c >= 0.0 { "+" } else { "−" }, c.abs())));
        let class = change.map_or("muted", |c| if c >= 0.0 { "gain" } else { "loss" });
        self.change.set_css_classes(&[class]);
        let spoken = [
            Some(symbol.to_string()),
            Some(name.to_string()),
            last.map(|l| format!("{l:.2}")),
            change.map(|c| format!("{} {:.2} percent", if c >= 0.0 { "up" } else { "down" }, c.abs())),
        ];
        let spoken: Vec<String> = spoken.into_iter().flatten().filter(|s| !s.is_empty()).collect();
        self.button.update_property(&[gtk::accessible::Property::Label(&spoken.join(", "))]);
    }
}
