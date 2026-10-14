//! Building a frame: render dirty entities, list surfaces, commit, encode.

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Instant;

use native_shell_core::host::{Measurer, TextMetrics, WidthMode};

use native_shell_core::desc::{CommitInput, Desc, ElementKind, EntityId, Surface, SurfaceKind, TextRun};
use native_shell_core::host::LogLevel;
use native_shell_core::protocol::{self, batch_flags, Command, Mutation, Prop, PropValue, ROOT_NODE};
use native_shell_core::style::{Arg, Resolved, StyleOp, TextStyle};
use rquickjs::{Function, Object, Value};

use super::{Instance, SurfaceRecord, SurfaceUpdate};
use crate::convert::{Converter, DualForm};
use crate::error::{script_error, ScriptError};
use crate::natives::{PendingCommand, Target};
use crate::sandbox::Phase;

/// The host's measurer, counting calls and time for `Metrics`.
struct TimedMeasurer<'a> {
    inner: &'a dyn Measurer,
    /// Texts measured (one per item of a batch) and the time spent.
    text: Cell<(u64, u64)>,
    /// `measure_text_batch` calls.
    batches: Cell<u64>,
    component: Cell<(u64, u64)>,
}

impl Measurer for TimedMeasurer<'_> {
    fn measure_text(&self, attributed_json: &str, width: WidthMode) -> TextMetrics {
        let started = Instant::now();
        let m = self.inner.measure_text(attributed_json, width);
        let (calls, ns) = self.text.get();
        self.text.set((calls + 1, ns + started.elapsed().as_nanos() as u64));
        m
    }

    fn measure_text_batch(&self, requests: &[(&str, WidthMode)]) -> Vec<TextMetrics> {
        let started = Instant::now();
        let m = self.inner.measure_text_batch(requests);
        let (calls, ns) = self.text.get();
        self.text.set((calls + requests.len() as u64, ns + started.elapsed().as_nanos() as u64));
        self.batches.set(self.batches.get() + 1);
        m
    }

    fn measure_component(&self, name: &str, props_json: &str, width: WidthMode) -> Option<(f32, f32)> {
        let started = Instant::now();
        let m = self.inner.measure_component(name, props_json, width);
        let (calls, ns) = self.component.get();
        self.component.set((calls + 1, ns + started.elapsed().as_nanos() as u64));
        m
    }
}

/// Engine-drawn chains: the surface wrapper, the error banner, the error page.
const CONTENT: &[&str] = &["size_full"];
const BANNER: &[&str] = &["absolute", "left_0", "right_0", "p_3"];
const ERROR_PAGE: &[&str] = &["size_full", "gap_3", "px_6"];
/// Payload of `PresentPage` / `DismissPage`.
const ANIMATED: &str = r#"{"animated":true}"#;

impl Instance {
    pub(super) fn frame(&mut self) {
        let theme_changed = {
            let mut s = self.shared.borrow_mut();
            s.frame_requested = false;
            std::mem::take(&mut s.theme_changed)
        };
        if theme_changed {
            self.styles.clear();
            if let Some(fg) = self.shared.borrow().theme.color("foreground") {
                self.tree.set_default_text_color(fg);
            }
            self.tree.clear_measure_cache();
            self.force_commit = true;
            let r = self.call(Phase::Task, "invalidate_all", |_, f| f.call::<_, ()>(()));
            self.log_failure("theme change", r);
        }
        let mut changed = std::mem::take(&mut self.force_commit);

        if self.engine.is_some() {
            let work = self.call(Phase::Task, "take_work", |_, f| {
                let o: Object<'_> = f.call(())?;
                Ok((o.get::<_, Vec<u32>>("dirty")?, o.get::<_, Vec<u32>>("released")?))
            });
            match work {
                Ok((dirty, released)) => {
                    for id in released {
                        self.entities.remove(&id);
                        self.errors.remove(&format!("e{id}"));
                        self.list_owner.retain(|_, owner| *owner != id);
                        changed = true;
                    }
                    for id in dirty {
                        self.render_entity(id);
                        changed = true;
                    }
                }
                Err(e) => self.host.log(LogLevel::Error, &format!("frame failed: {e}")),
            }
            changed |= self.update_surfaces();
        }

        let theme_tokens = self.theme_tokens();
        let has_commands = !self.shared.borrow().commands.is_empty();
        if !changed && !has_commands && theme_tokens.is_none() {
            return;
        }
        let surfaces = self.build_surfaces();
        let dismissals = self.page_dismissals();
        let started = Instant::now();
        let measurer = TimedMeasurer {
            inner: &*self.host,
            text: Cell::new((0, 0)),
            batches: Cell::new(0),
            component: Cell::new((0, 0)),
        };
        let mutations = self.tree.commit(&CommitInput { surfaces: &surfaces, entities: &self.entities }, &measurer);
        self.metrics.commit_ns += started.elapsed().as_nanos() as u64;
        let (calls, ns) = measurer.text.get();
        self.metrics.measure_text_calls += calls;
        self.metrics.measure_text_ns += ns;
        self.metrics.measure_text_batches += measurer.batches.get();
        let (calls, ns) = measurer.component.get();
        self.metrics.measure_component_calls += calls;
        self.metrics.measure_component_ns += ns;
        // Native views come and go only with Create/Delete: count them then, not on every read.
        if mutations.iter().any(|m| matches!(m, Mutation::Create { .. } | Mutation::Delete { .. })) {
            self.metrics.native_nodes = self.tree.native_count();
        }
        self.refresh_list_windows();
        let mut commands = self.commands();
        if let Some(json) = theme_tokens {
            // Before the commands: a toast in this batch already draws in the new colors.
            commands.insert(
                0,
                Mutation::SetProp { node: ROOT_NODE, prop: Prop::ThemeTokens, value: PropValue::Json(json) },
            );
        }
        if mutations.is_empty() && commands.is_empty() && dismissals.is_empty() {
            return;
        }
        let started = Instant::now();
        let flags = self.flags(&mutations);
        self.seq += 1;
        let mut encoder = protocol::Encoder::new();
        encoder.push(&Mutation::BeginBatch { seq: self.seq, flags });
        for m in dismissals.iter().chain(&mutations).chain(&commands) {
            encoder.push(m);
        }
        encoder.push(&Mutation::EndBatch);
        let bytes = encoder.finish();
        self.metrics.batch_bytes = bytes.len() as u64;
        self.batches.push_back(bytes);
        if self.seq == 1 {
            self.metrics.boot_ns = self.created.elapsed().as_nanos() as u64;
        }
        self.metrics.encode_ns += started.elapsed().as_nanos() as u64;
        self.metrics.batches += 1;
    }

    /// The resolved theme as `ThemeTokens` JSON (every color as an ARGB integer,
    /// plus `appearance`) when it differs from what the renderer last got; it
    /// is then recorded as sent.
    fn theme_tokens(&mut self) -> Option<String> {
        let json = {
            let s = self.shared.borrow();
            let mut o: serde_json::Map<String, serde_json::Value> =
                s.theme.colors.iter().map(|(k, v)| (k.clone(), serde_json::Value::from(*v))).collect();
            let appearance = if s.theme.is_dark() { "dark" } else { "light" };
            o.insert("appearance".into(), appearance.into());
            serde_json::Value::Object(o).to_string()
        };
        if self.sent_theme_tokens.as_deref() == Some(json.as_str()) {
            return None;
        }
        self.sent_theme_tokens = Some(json.clone());
        Some(json)
    }

    /// Converts what `scope` (a view, or an overlay's key) rendered.
    fn convert<'js>(&mut self, value: Value<'js>, scope: &str) -> (Result<Desc, String>, Vec<String>) {
        let theme = self.shared.borrow().theme.clone();
        let mut conv = Converter {
            styles: &mut self.styles,
            assets: &mut self.assets,
            warnings: Vec::new(),
            theme: &theme,
            base: self.base_style.clone(),
            lists: Vec::new(),
            nodes: 0,
            dual: Vec::new(),
        };
        let started = Instant::now();
        let r = conv.root(value);
        self.metrics.convert_ns += started.elapsed().as_nanos() as u64;
        self.metrics.converted_nodes += conv.nodes;
        let (lists, dual, warnings) = (conv.lists, conv.dual, conv.warnings);
        for w in warnings {
            self.host.log(LogLevel::Warn, &format!("native-shell: {w}"));
        }
        if self.dev && r.is_ok() {
            self.check_dual_forms(scope, dual);
        }
        (r, lists)
    }

    /// Dev mode: warns when a keyed switch, checkbox, radio or progress went from
    /// its native form to the drawn one (or back) since `scope` last rendered. The
    /// node is recreated when that happens, so a tap in flight is lost.
    fn check_dual_forms(&mut self, scope: &str, forms: Vec<DualForm>) {
        for f in forms.iter().filter(|f| f.empty) {
            if self.warned_empty.insert(format!("{scope}/{}", f.key)) {
                self.host.log(
                    LogLevel::Warn,
                    &format!(
                        "native-shell: control `{}` is drawn (it has a style that draws a box) but has no \
                         children, so nothing shows its state; give it children (a check mark, a track), or \
                         drop the box styles to use the native control",
                        f.key
                    ),
                );
            }
        }
        let now: HashMap<String, bool> = forms.into_iter().map(|f| (f.key, f.drawn)).collect();
        if let Some(before) = self.dual_forms.get(scope) {
            for (key, drawn) in &now {
                if before.get(key).is_some_and(|was| was != drawn) {
                    let (from, to) = if *drawn { ("native", "drawn") } else { ("drawn", "native") };
                    self.host.log(
                        LogLevel::Warn,
                        &format!(
                            "native-shell: control `{key}` changed from its {from} form to the {to} one between renders, \
                             so its view is recreated; give it `.headless()` or a box style to keep it drawn, or \
                             no children and only layout styles to keep it native"
                        ),
                    );
                }
            }
        }
        if now.is_empty() {
            self.dual_forms.remove(scope);
        } else {
            self.dual_forms.insert(scope.to_string(), now);
        }
    }

    fn render_entity(&mut self, id: EntityId) {
        self.metrics.renders += 1;
        let engine = match self.engine.clone() {
            Some(e) => e,
            None => return,
        };
        let budget = self.budget.clone();
        let context = self.context.clone();
        let mut render_ns = 0;
        let outcome: Option<Result<Desc, ScriptError>> = context.with(|ctx| {
            let render = || -> rquickjs::Result<Value<'_>> {
                let engine = engine.clone().restore(&ctx)?;
                let f: Function<'_> = engine.get("render")?;
                f.call((id,))
            };
            budget.start(Phase::Render);
            let started = Instant::now();
            let value = render();
            render_ns = started.elapsed().as_nanos() as u64;
            budget.stop();
            let value = match value {
                Ok(v) if v.is_null() => return None,
                Ok(v) => v,
                Err(rquickjs::Error::Exception) => return Some(Err(script_error(&ctx.catch(), budget.take_trip()))),
                Err(e) => return Some(Err(ScriptError { file: None, line: None, message: e.to_string() })),
            };
            let (desc, lists) = self.convert(value, &format!("e{id}"));
            for list in lists {
                self.list_owner.insert(list, id);
            }
            Some(desc.map_err(|message| ScriptError { file: None, line: None, message }))
        });
        self.metrics.render_ns += render_ns;
        let Some(outcome) = outcome else { return };
        let ok = outcome.is_ok();
        let settle = self.call(Phase::Task, "settle", |_, f| f.call::<_, ()>((format!("e{id}"), ok)));
        self.log_failure("settle", settle);
        match outcome {
            Ok(desc) => {
                self.entities.insert(id, Rc::new(desc));
                self.errors.remove(&format!("e{id}"));
                if Some(id) == self.root {
                    self.fatal = None;
                }
            }
            Err(e) => {
                self.metrics.render_errors += 1;
                self.host.log(LogLevel::Error, &format!("render failed: {e}"));
                if Some(id) == self.root && !self.entities.contains_key(&id) {
                    self.host.error(&format!("native-shell: first render failed: {e}"));
                    self.fatal = Some(e);
                } else {
                    self.record_error(format!("e{id}"), e.to_string());
                    self.entities
                        .entry(id)
                        .or_insert_with(|| Rc::new(Desc::new(ElementKind::View, self.base_style.clone())));
                }
            }
        }
    }

    fn record_error(&mut self, owner: String, message: String) {
        self.error_serial += 1;
        self.errors.insert(owner, (self.error_serial, message));
    }

    /// Asks the prelude for the surface list; converts overlay contents that re-rendered.
    fn update_surfaces(&mut self) -> bool {
        let engine = match self.engine.clone() {
            Some(e) => e,
            None => return false,
        };
        let budget = self.budget.clone();
        let context = self.context.clone();
        let listed: Result<Option<SurfaceUpdate>, ScriptError> = context.with(|ctx| {
            let list = || -> rquickjs::Result<Value<'_>> {
                let engine = engine.clone().restore(&ctx)?;
                let f: Function<'_> = engine.get("surfaces")?;
                f.call(())
            };
            budget.start(Phase::Render);
            let list = list();
            budget.stop();
            let list = match list {
                Ok(v) => v,
                Err(rquickjs::Error::Exception) => return Err(script_error(&ctx.catch(), budget.take_trip())),
                Err(e) => return Err(ScriptError { file: None, line: None, message: e.to_string() }),
            };
            let Some(items) = list.into_array() else { return Ok(None) };
            let mut records = Vec::new();
            let mut converted = Vec::new();
            let mut owners = Vec::new();
            let bad =
                |e: rquickjs::Error| ScriptError { file: None, line: None, message: format!("surface list: {e}") };
            for item in items.iter::<Object<'_>>() {
                let item = item.map_err(bad)?;
                let key: String = item.get("key").map_err(bad)?;
                let kind: String = item.get("kind").map_err(bad)?;
                match kind.as_str() {
                    "page" => records.push(SurfaceRecord::Page {
                        entity: item.get::<_, f64>("entity").map_err(bad)? as EntityId,
                        title: item.get("title").map_err(bad)?,
                        can_go_back: item.get("can_go_back").map_err(bad)?,
                        nav_bar_hidden: item.get("nav_bar_hidden").map_err(bad)?,
                        key,
                    }),
                    "alert" => records.push(SurfaceRecord::Alert { spec: item.get("spec").map_err(bad)?, key }),
                    _ => {
                        let opener = item.get::<_, f64>("opener").map_err(bad)? as EntityId;
                        let el: Value<'_> = item.get("el").map_err(bad)?;
                        let error: Option<String> = item.get("error").map_err(bad)?;
                        if let Some(error) = error {
                            converted.push((key.clone(), Err(error)));
                        } else if !el.is_null() {
                            let (desc, lists) = self.convert(el, &key);
                            owners.extend(lists.into_iter().map(|l| (l, opener)));
                            converted.push((key.clone(), desc));
                        }
                        records.push(SurfaceRecord::Overlay {
                            sheet: (kind == "sheet").then(|| item.get("detents").unwrap_or_default()),
                            dismissable: item.get("dismissable").unwrap_or(true),
                            key,
                        });
                    }
                }
            }
            Ok(Some(SurfaceUpdate { records, converted, owners }))
        });
        let SurfaceUpdate { records, converted, owners } = match listed {
            Ok(Some(x)) => x,
            Ok(None) => return false,
            Err(e) => {
                self.host.log(LogLevel::Error, &format!("surfaces failed: {e}"));
                return false;
            }
        };
        self.list_owner.extend(owners);
        for (key, desc) in converted {
            let ok = desc.is_ok();
            let settle = self.call(Phase::Task, "settle", |_, f| f.call::<_, ()>((key.clone(), ok)));
            self.log_failure("settle", settle);
            match desc {
                Ok(d) => {
                    self.overlay_content.insert(key.clone(), d);
                    self.errors.remove(&key);
                }
                Err(e) => {
                    self.metrics.render_errors += 1;
                    self.host.log(LogLevel::Error, &format!("overlay render failed: {e}"));
                    self.record_error(key, e);
                }
            }
        }
        let live: HashSet<&str> = records.iter().map(SurfaceRecord::key).collect();
        self.overlay_content.retain(|k, _| live.contains(k.as_str()));
        // Entity errors (`e<id>`) go when the entity renders or is released; overlay errors with the overlay.
        self.errors.retain(|k, _| k.starts_with('e') || live.contains(k.as_str()));
        self.surfaces = records;
        true
    }

    fn style(&mut self, names: &[&str], params: &[(&str, Arg)]) -> Rc<Resolved> {
        let mut ops: Vec<StyleOp> = names.iter().map(|n| StyleOp::new(n, Vec::new())).collect();
        ops.extend(params.iter().map(|(n, a)| StyleOp::new(n, vec![a.clone()])));
        let key = format!("engine:{}", native_shell_core::style::chain_key(&ops));
        let theme = self.shared.borrow().theme.clone();
        self.styles.resolve(&key, &ops, &theme).unwrap_or_else(|_| self.base_style.clone())
    }

    fn message_text(&self, text: &str, size: f32, weight: u16, token: &str) -> Desc {
        let color = self.shared.borrow().theme.color(token);
        let run = TextRun {
            text: text.to_string(),
            style: TextStyle { size: Some(size), weight: Some(weight), color, ..TextStyle::default() },
            url: None,
        };
        Desc::new(ElementKind::Text(vec![run]), self.base_style.clone())
    }

    fn content(&mut self, entity: EntityId, banner: Option<String>) -> Desc {
        let mut d = Desc::new(ElementKind::View, self.style(CONTENT, &[]));
        d.children.push(Desc::new(ElementKind::Entity(entity), self.base_style.clone()));
        if let Some(message) = banner {
            let top = self.shared.borrow().env.safe_area[0];
            let style = self.style(BANNER, &[("top", Arg::Num(top as f64)), ("bg", Arg::Str("destructive".into()))]);
            let mut b = Desc::new(ElementKind::View, style);
            b.props.push((Prop::A11yRole, PropValue::I32(protocol::A11yRole::Alert as i32)));
            b.props.push((Prop::TestId, PropValue::Str("nsh-error-banner".into())));
            b.children.push(self.message_text(&message, 13.0, 500, "destructive_foreground"));
            d.children.push(b);
        }
        d
    }

    fn error_page(&mut self, e: &ScriptError) -> Desc {
        let top = self.shared.borrow().env.safe_area[0];
        let style =
            self.style(ERROR_PAGE, &[("pt", Arg::Num(top as f64 + 24.0)), ("bg", Arg::Str("background".into()))]);
        let mut d = Desc::new(ElementKind::View, style);
        d.props.push((Prop::TestId, PropValue::Str("nsh-error-page".into())));
        d.children.push(self.message_text("This program failed to start", 20.0, 700, "foreground"));
        d.children.push(self.message_text(&e.to_string(), 14.0, 400, "destructive"));
        d
    }

    fn build_surfaces(&mut self) -> Vec<Surface> {
        let banner = self.errors.values().max_by_key(|(serial, _)| *serial).map(|(_, m)| m.clone());
        let root_content = match (&self.fatal, self.root) {
            (Some(e), _) => {
                let e = e.clone();
                self.error_page(&e)
            }
            (None, Some(root)) => self.content(root, banner),
            (None, None) => Desc::new(ElementKind::View, self.base_style.clone()),
        };
        let mut out = vec![Surface { key: "root".into(), kind: SurfaceKind::Root, content: root_content }];
        let records = std::mem::take(&mut self.surfaces);
        for r in &records {
            let surface = match r {
                SurfaceRecord::Page { key, entity, title, can_go_back, nav_bar_hidden, .. } => Surface {
                    key: key.clone(),
                    kind: SurfaceKind::Page {
                        title: title.clone(),
                        can_go_back: *can_go_back,
                        nav_bar_hidden: *nav_bar_hidden,
                    },
                    content: self.content(*entity, None),
                },
                SurfaceRecord::Overlay { key, sheet, dismissable } => Surface {
                    key: key.clone(),
                    kind: match sheet {
                        Some(detents) => SurfaceKind::Sheet { detents: detents.clone(), dismissable: *dismissable },
                        None => SurfaceKind::Dialog { dismissable: *dismissable },
                    },
                    content: self
                        .overlay_content
                        .get(key)
                        .cloned()
                        .unwrap_or_else(|| Desc::new(ElementKind::View, self.base_style.clone())),
                },
                SurfaceRecord::Alert { key, spec } => Surface {
                    key: key.clone(),
                    kind: SurfaceKind::Alert { spec_json: spec.clone() },
                    content: Desc::new(ElementKind::View, self.base_style.clone()),
                },
            };
            out.push(surface);
        }
        self.surfaces = records;
        out
    }

    fn refresh_list_windows(&mut self) {
        let mut s = self.shared.borrow_mut();
        s.list_windows.retain(|id, _| self.list_owner.contains_key(id));
        for id in self.list_owner.keys() {
            s.list_windows.insert(id.clone(), self.tree.list_window(id));
        }
    }

    /// Pages the script popped since the last frame: `DismissPage` goes before the tree's
    /// `Delete` in the same batch, while the node still exists (pages the user dismissed
    /// natively are already gone on screen and get none).
    fn page_dismissals(&mut self) -> Vec<Mutation> {
        let live: HashSet<&str> = self
            .surfaces
            .iter()
            .filter_map(|r| match r {
                SurfaceRecord::Page { key, .. } => Some(key.as_str()),
                _ => None,
            })
            .collect();
        let gone: Vec<String> = self.presented.iter().filter(|k| !live.contains(k.as_str())).cloned().collect();
        let mut out = Vec::new();
        for key in gone {
            self.presented.remove(&key);
            if self.native_dismissed.remove(&key) {
                continue;
            }
            if let Some(node) = self.tree.surface_node(&key) {
                out.push(Mutation::Command { node, command: Command::DismissPage, payload: ANIMATED.into() });
            }
        }
        self.native_dismissed.clear();
        out
    }

    /// A `ScrollTo` the script gave as `{"page": n}` (a paged scroll, which
    /// cannot know its width) becomes the offset of that page now that the
    /// frame is laid out: `n` viewport widths.
    fn page_offset(&self, node: u32, command: Command, payload: String) -> String {
        if command != Command::ScrollTo {
            return payload;
        }
        let Ok(mut json) = serde_json::from_str::<serde_json::Value>(&payload) else { return payload };
        let (Some(page), Some(frame)) = (json.get("page").and_then(|p| p.as_f64()), self.tree.frame(node)) else { return payload };
        if let Some(obj) = json.as_object_mut() {
            obj.remove("page");
            obj.insert("x".into(), serde_json::json!(page * f64::from(frame.width)));
            obj.insert("y".into(), serde_json::json!(0));
        }
        json.to_string()
    }

    /// `PresentPage` for pages pushed this frame (none in the initial batch), then the
    /// script's queued commands, resolved to nodes.
    fn commands(&mut self) -> Vec<Mutation> {
        let mut out = Vec::new();
        for r in &self.surfaces {
            if let SurfaceRecord::Page { key, .. } = r {
                if !self.presented.insert(key.clone()) || self.seq == 0 {
                    continue;
                }
                if let Some(node) = self.tree.surface_node(key) {
                    out.push(Mutation::Command { node, command: Command::PresentPage, payload: ANIMATED.into() });
                }
            }
        }
        let pending: Vec<PendingCommand> = std::mem::take(&mut self.shared.borrow_mut().commands);
        for c in pending {
            let node = match &c.target {
                Target::Root => Some(ROOT_NODE),
                Target::Ref(r) => self.tree.node_for_ref(r),
            };
            match node {
                Some(node) => {
                    let payload = self.page_offset(node, c.command, c.payload);
                    out.push(Mutation::Command { node, command: c.command, payload })
                }
                None => self.host.log(
                    LogLevel::Warn,
                    &format!("native-shell: {} dropped: nothing rendered with id {:?}", c.command.name(), c.target),
                ),
            }
        }
        out
    }

    /// INITIAL on the first batch; ANIMATED when a node with a transition moves or fades.
    fn flags(&mut self, mutations: &[Mutation]) -> u32 {
        let mut created = HashSet::new();
        let mut animated = false;
        for m in mutations {
            match m {
                Mutation::Create { node, .. } => {
                    created.insert(*node);
                }
                Mutation::Delete { node } => {
                    self.animated.remove(node);
                }
                Mutation::SetProp { node, prop: Prop::Animation, value } => {
                    if *value == PropValue::Null {
                        self.animated.remove(node);
                    } else {
                        self.animated.insert(*node);
                    }
                }
                _ => {}
            }
        }
        for m in mutations {
            if let Mutation::SetFrame { node, .. } | Mutation::SetProp { node, prop: Prop::Opacity, .. } = m {
                if self.animated.contains(node) && !created.contains(node) {
                    animated = true;
                }
            }
        }
        let mut flags = 0;
        if self.seq == 0 {
            flags |= batch_flags::INITIAL;
        }
        if animated {
            flags |= batch_flags::ANIMATED;
        }
        flags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use native_shell_core::theme::{Appearance, Theme};

    #[test]
    fn engine_chains_are_in_the_vocabulary() {
        let theme = Theme::default_for(Appearance::Light);
        for chain in [CONTENT, BANNER, ERROR_PAGE] {
            let ops: Vec<StyleOp> = chain.iter().map(|n| StyleOp::new(n, Vec::new())).collect();
            native_shell_core::style::resolve(&ops, &theme).unwrap_or_else(|e| panic!("{chain:?}: {e}"));
        }
    }
}
