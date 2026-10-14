//! `svg(path)`: the program's SVG files turned into `Shape` path data.
//!
//! gpui draws an svg as a mask filled with one color (its text color), so what
//! an icon needs is its geometry, not a full SVG renderer. This parses the
//! subset icon sets use — `<svg>` (`viewBox`, `width`, `height`), `<g>` with
//! `translate`/`scale` transforms, `<path>`, `<circle>`, `<ellipse>`, `<rect>`
//! (with `rx`/`ry`), `<line>`, `<polyline>`, `<polygon>`, and whether each is
//! filled and/or stroked (attributes or `style`, inherited) — into absolute
//! path data in view-box units. Gradients, text, masks, clips and `<use>` are
//! skipped. Paths sharing a paint (fill, stroke width) are merged, so most
//! icons come out as one path.

/// One paint: the merged path data of every element painted the same way.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Layer {
    pub path: String,
    pub fill: bool,
    /// Filled with the even-odd rule (`fill-rule="evenodd"`) rather than nonzero.
    pub even_odd: bool,
    /// Stroke width in view-box units, when stroked.
    pub stroke: Option<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Icon {
    /// `[min_x, min_y, width, height]`.
    pub view_box: [f32; 4],
    pub layers: Vec<Layer>,
}

/// Parses an SVG document; errors name what was wrong.
pub(crate) fn parse(text: &str) -> Result<Icon, String> {
    let mut parser = Parser { icon: None, layers: Vec::new(), stack: Vec::new(), skip: 0 };
    for tag in Tags::new(text) {
        parser.tag(tag?)?;
    }
    let mut icon = parser.icon.ok_or("no <svg> element")?;
    icon.layers = parser.layers;
    Ok(icon)
}

/// Paint and transform in effect for an element (inherited from its parents).
#[derive(Debug, Clone, Copy)]
struct Paint {
    fill: bool,
    /// `fill-rule` and `clip-rule` as given (true = evenodd); fill-rule wins.
    /// Icon sets often set both on a path, and some only `clip-rule`.
    fill_rule: Option<bool>,
    clip_rule: Option<bool>,
    stroke: bool,
    stroke_width: f32,
    transform: Affine,
}

/// `x' = sx·x + tx`, `y' = sy·y + ty`: translate and scale only (arcs stay arcs).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Affine {
    sx: f32,
    sy: f32,
    tx: f32,
    ty: f32,
}

impl Affine {
    const IDENTITY: Affine = Affine { sx: 1.0, sy: 1.0, tx: 0.0, ty: 0.0 };

    /// `self` applied after `inner`.
    fn then(self, inner: Affine) -> Affine {
        Affine {
            sx: self.sx * inner.sx,
            sy: self.sy * inner.sy,
            tx: self.sx * inner.tx + self.tx,
            ty: self.sy * inner.ty + self.ty,
        }
    }

    fn point(&self, x: f32, y: f32) -> (f32, f32) {
        (self.sx * x + self.tx, self.sy * y + self.ty)
    }
}

struct Parser {
    icon: Option<Icon>,
    layers: Vec<Layer>,
    /// The paint of every open element.
    stack: Vec<Paint>,
    /// Depth inside a skipped subtree (`<defs>`, `<text>`, …).
    skip: u32,
}

/// Elements whose content is not drawn geometry.
const SKIPPED: &[&str] = &[
    "defs",
    "clipPath",
    "mask",
    "symbol",
    "linearGradient",
    "radialGradient",
    "pattern",
    "filter",
    "marker",
    "text",
    "style",
    "title",
    "desc",
    "metadata",
    "script",
    "foreignObject",
];

impl Parser {
    fn tag(&mut self, tag: Tag<'_>) -> Result<(), String> {
        let Tag { name, attrs, open, close } = tag;
        if self.skip > 0 {
            if open && !close {
                self.skip += 1;
            } else if !open {
                self.skip -= 1;
            }
            return Ok(());
        }
        if !open {
            self.stack.pop();
            return Ok(());
        }
        if SKIPPED.contains(&name) {
            if !close {
                self.skip = 1;
            }
            return Ok(());
        }
        let parent = self.stack.last().copied();
        if name == "svg" && self.icon.is_none() {
            self.icon = Some(Icon { view_box: view_box(&attrs)?, layers: Vec::new() });
        } else if self.icon.is_none() {
            return Err(format!("<{name}> outside <svg>"));
        }
        let mut paint = parent.unwrap_or(Paint {
            fill: true,
            fill_rule: None,
            clip_rule: None,
            stroke: false,
            stroke_width: 1.0,
            transform: Affine::IDENTITY,
        });
        apply_paint(&mut paint, &attrs)?;
        if let Some(path) = self.geometry(name, &attrs, paint.transform)? {
            self.add(path, paint);
        }
        if !close {
            self.stack.push(paint);
        }
        Ok(())
    }

    fn geometry(&mut self, name: &str, a: &Attrs<'_>, t: Affine) -> Result<Option<String>, String> {
        let num = |k: &str| a.get(k).map(|v| length(v).ok_or_else(|| format!("<{name}> {k}=\"{v}\": not a number")));
        let n = |k: &str| num(k).transpose().map(|v| v.unwrap_or(0.0));
        let d = match name {
            "path" => a.get("d").unwrap_or("").to_string(),
            "circle" => {
                let r = n("r")?;
                ellipse(n("cx")?, n("cy")?, r, r)
            }
            "ellipse" => ellipse(n("cx")?, n("cy")?, n("rx")?, n("ry")?),
            "rect" => {
                let (x, y, w, h) = (n("x")?, n("y")?, n("width")?, n("height")?);
                let (rx, ry) = match (num("rx").transpose()?, num("ry").transpose()?) {
                    (Some(rx), Some(ry)) => (rx, ry),
                    (Some(r), None) | (None, Some(r)) => (r, r),
                    (None, None) => (0.0, 0.0),
                };
                rect(x, y, w, h, rx.clamp(0.0, w / 2.0), ry.clamp(0.0, h / 2.0))
            }
            "line" => format!("M{} {}L{} {}", n("x1")?, n("y1")?, n("x2")?, n("y2")?),
            "polyline" | "polygon" => {
                let pts = numbers(a.get("points").unwrap_or(""))
                    .ok_or_else(|| format!("<{name}> points: not a list of numbers"))?;
                let mut d = String::new();
                for (i, [x, y]) in pts.as_chunks::<2>().0.iter().enumerate() {
                    d += &format!("{}{x} {y}", if i == 0 { 'M' } else { 'L' });
                }
                if name == "polygon" && !d.is_empty() {
                    d.push('Z');
                }
                d
            }
            _ => return Ok(None),
        };
        if d.trim().is_empty() {
            return Ok(None);
        }
        normalize(&d, t).map(Some).map_err(|e| format!("<{name}>: {e}"))
    }

    fn add(&mut self, path: String, paint: Paint) {
        if !paint.fill && !paint.stroke {
            return;
        }
        let scale = (paint.transform.sx * paint.transform.sy).abs().sqrt();
        let stroke = paint.stroke.then_some(paint.stroke_width * scale);
        let even_odd = paint.fill && paint.fill_rule.or(paint.clip_rule).unwrap_or(false);
        match self.layers.iter_mut().find(|l| l.fill == paint.fill && l.even_odd == even_odd && l.stroke == stroke) {
            Some(l) => l.path += &path,
            None => self.layers.push(Layer { path, fill: paint.fill, even_odd, stroke }),
        }
    }
}

fn view_box(a: &Attrs<'_>) -> Result<[f32; 4], String> {
    if let Some(v) = a.get("viewBox") {
        return match numbers(v).as_deref() {
            Some([x, y, w, h]) if *w > 0.0 && *h > 0.0 => Ok([*x, *y, *w, *h]),
            _ => Err(format!("viewBox=\"{v}\": expected four numbers with a positive size")),
        };
    }
    match (a.get("width").and_then(length), a.get("height").and_then(length)) {
        (Some(w), Some(h)) if w > 0.0 && h > 0.0 => Ok([0.0, 0.0, w, h]),
        _ => Err("<svg> needs a viewBox, or a width and a height".into()),
    }
}

/// Presentation attributes, then the `style` attribute (which wins, as in CSS).
fn apply_paint(p: &mut Paint, a: &Attrs<'_>) -> Result<(), String> {
    let mut set = |k: &str, v: &str| -> Result<(), String> {
        let v = v.trim();
        match k {
            "fill" => p.fill = v != "none",
            "fill-rule" | "clip-rule" => {
                let even_odd = match v {
                    "evenodd" => true,
                    "nonzero" => false,
                    _ => return Err(format!("{k}=\"{v}\": use nonzero or evenodd")),
                };
                *if k == "fill-rule" { &mut p.fill_rule } else { &mut p.clip_rule } = Some(even_odd);
            }
            "stroke" => p.stroke = v != "none",
            "stroke-width" => {
                p.stroke_width = length(v).ok_or_else(|| format!("stroke-width=\"{v}\": not a number"))?;
            }
            "transform" => p.transform = p.transform.then(transform(v)?),
            _ => {}
        }
        Ok(())
    };
    for (k, v) in &a.0 {
        set(k, v)?;
    }
    if let Some(style) = a.get("style") {
        for decl in style.split(';') {
            if let Some((k, v)) = decl.split_once(':') {
                set(k.trim(), v)?;
            }
        }
    }
    Ok(())
}

/// `translate(...)`, `scale(...)` and axis-aligned `matrix(a 0 0 d e f)`, in sequence.
fn transform(v: &str) -> Result<Affine, String> {
    let mut out = Affine::IDENTITY;
    let mut rest = v.trim();
    while !rest.is_empty() {
        let (name, after) = rest.split_once('(').ok_or_else(|| format!("transform=\"{v}\": expected name(...)"))?;
        let (args, after) = after.split_once(')').ok_or_else(|| format!("transform=\"{v}\": missing `)`"))?;
        let n = numbers(args).ok_or_else(|| format!("transform=\"{v}\": bad numbers"))?;
        let t = match (name.trim(), n.as_slice()) {
            ("translate", [x]) => Affine { tx: *x, ..Affine::IDENTITY },
            ("translate", [x, y]) => Affine { tx: *x, ty: *y, ..Affine::IDENTITY },
            ("scale", [s]) => Affine { sx: *s, sy: *s, ..Affine::IDENTITY },
            ("scale", [x, y]) => Affine { sx: *x, sy: *y, ..Affine::IDENTITY },
            ("matrix", [a, b, c, d, e, f]) if *b == 0.0 && *c == 0.0 => Affine { sx: *a, sy: *d, tx: *e, ty: *f },
            (other, _) => return Err(format!("transform `{}`: only translate and scale are supported", other.trim())),
        };
        out = out.then(t);
        rest = after.trim_start_matches(|c: char| c.is_whitespace() || c == ',');
    }
    Ok(out)
}

fn ellipse(cx: f32, cy: f32, rx: f32, ry: f32) -> String {
    if rx <= 0.0 || ry <= 0.0 {
        return String::new();
    }
    format!("M{} {cy}A{rx} {ry} 0 1 0 {} {cy}A{rx} {ry} 0 1 0 {} {cy}Z", cx - rx, cx + rx, cx - rx)
}

fn rect(x: f32, y: f32, w: f32, h: f32, rx: f32, ry: f32) -> String {
    if w <= 0.0 || h <= 0.0 {
        return String::new();
    }
    if rx <= 0.0 || ry <= 0.0 {
        return format!("M{x} {y}H{}V{}H{x}Z", x + w, y + h);
    }
    let (r, b) = (x + w, y + h);
    format!(
        "M{} {y}H{}A{rx} {ry} 0 0 1 {r} {}V{}A{rx} {ry} 0 0 1 {} {b}H{}A{rx} {ry} 0 0 1 {x} {}V{}A{rx} {ry} 0 0 1 {} {y}Z",
        x + rx,
        r - rx,
        y + ry,
        b - ry,
        r - rx,
        x + rx,
        b - ry,
        y + ry,
        x + rx
    )
}

/// A number with an optional `px` unit (other units are not supported).
fn length(v: &str) -> Option<f32> {
    v.trim().trim_end_matches("px").trim().parse().ok().filter(|n: &f32| n.is_finite())
}

fn numbers(s: &str) -> Option<Vec<f32>> {
    let mut lexer = Lexer { s: s.as_bytes(), i: 0 };
    let mut out = Vec::new();
    loop {
        lexer.separators();
        if lexer.i >= lexer.s.len() {
            return Some(out);
        }
        out.push(lexer.number()?);
    }
}

// Path data -------------------------------------------------------------------

/// Rewrites path data as absolute `M L C Q A Z` commands with `t` applied.
fn normalize(d: &str, t: Affine) -> Result<String, String> {
    let mut lx = Lexer { s: d.as_bytes(), i: 0 };
    let mut out = String::with_capacity(d.len());
    let (mut cur, mut start) = ((0.0f32, 0.0f32), (0.0f32, 0.0f32));
    // The last control point of a C/S (cubic) or Q/T (quadratic) command, for reflection.
    let mut last_cubic: Option<(f32, f32)> = None;
    let mut last_quad: Option<(f32, f32)> = None;
    let mut cmd: Option<u8> = None;
    let emit = |out: &mut String, c: char, pts: &[(f32, f32)]| {
        out.push(c);
        for (i, (x, y)) in pts.iter().enumerate() {
            let (x, y) = t.point(*x, *y);
            if i > 0 {
                out.push(' ');
            }
            out.push_str(&fmt(x));
            out.push(' ');
            out.push_str(&fmt(y));
        }
    };
    loop {
        lx.separators();
        if lx.i >= lx.s.len() {
            break;
        }
        let c = lx.s[lx.i];
        if c.is_ascii_alphabetic() {
            lx.i += 1;
            cmd = Some(c);
        } else if cmd.is_none() {
            return Err(format!("path data must start with a command, at `{}`", lx.rest()));
        }
        let c = cmd.ok_or("path data without a command")?;
        let rel = c.is_ascii_lowercase();
        let base = if rel { cur } else { (0.0, 0.0) };
        let pt = |lx: &mut Lexer<'_>| -> Result<(f32, f32), String> {
            let x = lx.arg()?;
            let y = lx.arg()?;
            Ok((base.0 + x, base.1 + y))
        };
        let (mut cubic, mut quad) = (None, None);
        match c.to_ascii_uppercase() {
            b'M' => {
                cur = pt(&mut lx)?;
                start = cur;
                emit(&mut out, 'M', &[cur]);
                // Further pairs after a moveto are linetos.
                cmd = Some(if rel { b'l' } else { b'L' });
            }
            b'L' => {
                cur = pt(&mut lx)?;
                emit(&mut out, 'L', &[cur]);
            }
            b'H' => {
                let x = lx.arg()?;
                cur.0 = if rel { cur.0 + x } else { x };
                emit(&mut out, 'L', &[cur]);
            }
            b'V' => {
                let y = lx.arg()?;
                cur.1 = if rel { cur.1 + y } else { y };
                emit(&mut out, 'L', &[cur]);
            }
            b'C' => {
                let (c1, c2, p) = (pt(&mut lx)?, pt(&mut lx)?, pt(&mut lx)?);
                emit(&mut out, 'C', &[c1, c2, p]);
                cubic = Some(c2);
                cur = p;
            }
            b'S' => {
                let c1 = reflect(last_cubic, cur);
                let (c2, p) = (pt(&mut lx)?, pt(&mut lx)?);
                emit(&mut out, 'C', &[c1, c2, p]);
                cubic = Some(c2);
                cur = p;
            }
            b'Q' => {
                let (c1, p) = (pt(&mut lx)?, pt(&mut lx)?);
                emit(&mut out, 'Q', &[c1, p]);
                quad = Some(c1);
                cur = p;
            }
            b'T' => {
                let c1 = reflect(last_quad, cur);
                let p = pt(&mut lx)?;
                emit(&mut out, 'Q', &[c1, p]);
                quad = Some(c1);
                cur = p;
            }
            b'A' => {
                let (rx, ry, rot) = (lx.arg()?, lx.arg()?, lx.arg()?);
                let (large, sweep) = (lx.flag()?, lx.flag()?);
                let p = pt(&mut lx)?;
                let (x, y) = t.point(p.0, p.1);
                // A mirroring scale reverses the direction of travel.
                let sweep = if (t.sx * t.sy) < 0.0 { !sweep } else { sweep };
                out.push_str(&format!(
                    "A{} {} {} {} {} {} {}",
                    fmt((rx * t.sx).abs()),
                    fmt((ry * t.sy).abs()),
                    fmt(rot),
                    u8::from(large),
                    u8::from(sweep),
                    fmt(x),
                    fmt(y)
                ));
                cur = p;
            }
            b'Z' => {
                out.push('Z');
                cur = start;
                // Z takes no arguments; a number after it needs a new command.
                cmd = None;
            }
            other => return Err(format!("unknown path command `{}`", other as char)),
        }
        last_cubic = cubic;
        last_quad = quad;
    }
    Ok(out)
}

fn reflect(control: Option<(f32, f32)>, cur: (f32, f32)) -> (f32, f32) {
    control.map_or(cur, |(x, y)| (2.0 * cur.0 - x, 2.0 * cur.1 - y))
}

/// At most three decimals, without trailing zeros.
fn fmt(v: f32) -> String {
    let r = (v * 1000.0).round() / 1000.0;
    let r = if r == 0.0 { 0.0 } else { r };
    format!("{r}")
}

struct Lexer<'a> {
    s: &'a [u8],
    i: usize,
}

impl Lexer<'_> {
    fn rest(&self) -> String {
        String::from_utf8_lossy(&self.s[self.i..self.s.len().min(self.i + 12)]).into_owned()
    }

    fn separators(&mut self) {
        while self.i < self.s.len() && (self.s[self.i].is_ascii_whitespace() || self.s[self.i] == b',') {
            self.i += 1;
        }
    }

    fn arg(&mut self) -> Result<f32, String> {
        self.separators();
        self.number().ok_or_else(|| format!("expected a number at `{}`", self.rest()))
    }

    /// An arc flag: a single `0` or `1`, which may be glued to what follows.
    fn flag(&mut self) -> Result<bool, String> {
        self.separators();
        match self.s.get(self.i) {
            Some(b'0') => {
                self.i += 1;
                Ok(false)
            }
            Some(b'1') => {
                self.i += 1;
                Ok(true)
            }
            _ => Err(format!("expected an arc flag at `{}`", self.rest())),
        }
    }

    /// `-1.5e3`, `.5`, and `1.5.5` read as `1.5` then `.5`, as SVG allows.
    fn number(&mut self) -> Option<f32> {
        let s = self.s;
        let begin = self.i;
        let mut i = self.i;
        if i < s.len() && (s[i] == b'+' || s[i] == b'-') {
            i += 1;
        }
        let digits = |i: &mut usize| {
            let from = *i;
            while *i < s.len() && s[*i].is_ascii_digit() {
                *i += 1;
            }
            *i > from
        };
        let mut any = digits(&mut i);
        if i < s.len() && s[i] == b'.' {
            i += 1;
            any |= digits(&mut i);
        }
        if !any {
            return None;
        }
        if i < s.len() && (s[i] == b'e' || s[i] == b'E') {
            let mut j = i + 1;
            if j < s.len() && (s[j] == b'+' || s[j] == b'-') {
                j += 1;
            }
            if digits(&mut j) {
                i = j;
            }
        }
        self.i = i;
        std::str::from_utf8(&s[begin..i]).ok()?.parse().ok().filter(|n: &f32| n.is_finite())
    }
}

// XML -------------------------------------------------------------------------

struct Attrs<'a>(Vec<(&'a str, String)>);

impl Attrs<'_> {
    fn get(&self, name: &str) -> Option<&str> {
        self.0.iter().find(|(k, _)| *k == name).map(|(_, v)| v.as_str())
    }
}

/// A start tag (`open`), an end tag (`!open`), or a self-closing one (`open && close`).
struct Tag<'a> {
    name: &'a str,
    attrs: Attrs<'a>,
    open: bool,
    close: bool,
}

/// The tags of an XML document; text, comments, declarations and CDATA are skipped.
struct Tags<'a> {
    s: &'a str,
    i: usize,
}

impl<'a> Tags<'a> {
    fn new(s: &'a str) -> Tags<'a> {
        Tags { s, i: 0 }
    }

    fn skip_to(&mut self, end: &str) -> Result<(), String> {
        match self.s[self.i..].find(end) {
            Some(n) => {
                self.i += n + end.len();
                Ok(())
            }
            None => Err(format!("unterminated markup, expected `{end}`")),
        }
    }

    fn tag(&mut self) -> Result<Tag<'a>, String> {
        let s = self.s;
        let bytes = s.as_bytes();
        let open = bytes.get(self.i) != Some(&b'/');
        if !open {
            self.i += 1;
        }
        let name_start = self.i;
        while self.i < s.len() && !bytes[self.i].is_ascii_whitespace() && !matches!(bytes[self.i], b'>' | b'/') {
            self.i += 1;
        }
        let name = &s[name_start..self.i];
        // `svg:path` and `path` are the same element.
        let name = name.rsplit(':').next().unwrap_or(name);
        let mut attrs = Vec::new();
        loop {
            while self.i < s.len() && bytes[self.i].is_ascii_whitespace() {
                self.i += 1;
            }
            match bytes.get(self.i) {
                None => return Err(format!("unterminated <{name}>")),
                Some(b'>') => {
                    self.i += 1;
                    return Ok(Tag { name, attrs: Attrs(attrs), open, close: !open });
                }
                Some(b'/') if bytes.get(self.i + 1) == Some(&b'>') => {
                    self.i += 2;
                    return Ok(Tag { name, attrs: Attrs(attrs), open, close: true });
                }
                _ => {}
            }
            let key_start = self.i;
            while self.i < s.len()
                && !matches!(bytes[self.i], b'=' | b'>' | b'/')
                && !bytes[self.i].is_ascii_whitespace()
            {
                self.i += 1;
            }
            let key = &s[key_start..self.i];
            if key.is_empty() {
                return Err(format!("<{name}>: malformed attribute"));
            }
            while self.i < s.len() && bytes[self.i].is_ascii_whitespace() {
                self.i += 1;
            }
            if bytes.get(self.i) != Some(&b'=') {
                attrs.push((key, String::new()));
                continue;
            }
            self.i += 1;
            while self.i < s.len() && bytes[self.i].is_ascii_whitespace() {
                self.i += 1;
            }
            let quote = *bytes.get(self.i).ok_or_else(|| format!("<{name}> {key}: missing value"))?;
            if quote != b'"' && quote != b'\'' {
                return Err(format!("<{name}> {key}: the value must be quoted"));
            }
            self.i += 1;
            let len = s[self.i..].find(quote as char).ok_or_else(|| format!("<{name}> {key}: unterminated value"))?;
            attrs.push((key, unescape(&s[self.i..self.i + len])));
            self.i += len + 1;
        }
    }
}

impl<'a> Iterator for Tags<'a> {
    type Item = Result<Tag<'a>, String>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let n = self.s[self.i..].find('<')?;
            self.i += n + 1;
            let rest = &self.s[self.i..];
            let skipped = if rest.starts_with("!--") {
                self.skip_to("-->")
            } else if rest.starts_with("![CDATA[") {
                self.skip_to("]]>")
            } else if rest.starts_with('?') || rest.starts_with('!') {
                self.skip_to(">")
            } else {
                return Some(self.tag());
            };
            if let Err(e) = skipped {
                return Some(Err(e));
            }
        }
    }
}

fn unescape(v: &str) -> String {
    if !v.contains('&') {
        return v.to_string();
    }
    const ENTITIES: [(&str, &str); 5] =
        [("&lt;", "<"), ("&gt;", ">"), ("&quot;", "\""), ("&apos;", "'"), ("&amp;", "&")];
    let mut out = v.to_string();
    for (from, to) in ENTITIES {
        out = out.replace(from, to);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stroked_icon_keeps_its_path_and_stroke_width() {
        let icon = parse(
            r#"<?xml version="1.0"?><!-- lucide --><svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 5v14M5 12h14"/></svg>"#,
        )
        .unwrap();
        assert_eq!(icon.view_box, [0.0, 0.0, 24.0, 24.0]);
        assert_eq!(
            icon.layers,
            vec![Layer { path: "M12 5L12 19M5 12L19 12".into(), fill: false, even_odd: false, stroke: Some(2.0) }]
        );
    }

    #[test]
    fn shapes_become_paths_and_groups_transform_them() {
        let icon = parse(
            r##"<svg width="20px" height="10"><g transform="translate(10 0) scale(2)"><rect x="1" y="1" width="2" height="2"/><circle cx="0" cy="0" r="1" style="fill: none; stroke: red; stroke-width: 0.5"/></g><line x1="0" y1="0" x2="4" y2="0" stroke="#000"/><polygon points="0,0 1,0 1,1"/></svg>"##,
        )
        .unwrap();
        assert_eq!(icon.view_box, [0.0, 0.0, 20.0, 10.0]);
        assert_eq!(icon.layers.len(), 3, "{:?}", icon.layers);
        assert_eq!(icon.layers[0].path, "M12 2L16 2L16 6L12 6ZM0 0L1 0L1 1Z");
        assert_eq!((icon.layers[0].fill, icon.layers[0].stroke), (true, None));
        assert_eq!(icon.layers[1].path, "M8 0A2 2 0 1 0 12 0A2 2 0 1 0 8 0Z");
        assert_eq!(icon.layers[1].stroke, Some(1.0), "stroke widths scale with the group");
        assert_eq!((icon.layers[2].fill, icon.layers[2].stroke), (true, Some(1.0)));
    }

    #[test]
    fn path_data_is_made_absolute() {
        let t = Affine::IDENTITY;
        assert_eq!(normalize("m1 1 2 0h1v1z l1-1", t).unwrap(), "M1 1L3 1L4 1L4 2ZL2 0");
        assert_eq!(normalize("M0 0c1 1 2 2 3 3s1 1 2 2", t).unwrap(), "M0 0C1 1 2 2 3 3C4 4 4 4 5 5");
        assert_eq!(normalize("M0 0q1 1 2 0t2 0", t).unwrap(), "M0 0Q1 1 2 0Q3 -1 4 0");
        assert_eq!(normalize("M1.5.5a1 1 0 01-1-1", t).unwrap(), "M1.5 0.5A1 1 0 0 1 0.5 -0.5");
        let mirror = Affine { sx: -1.0, ..Affine::IDENTITY };
        assert_eq!(normalize("M0 0A1 2 0 0 1 2 0", mirror).unwrap(), "M0 0A1 2 0 0 0 -2 0");
        assert!(normalize("5 5", t).is_err());
        assert!(normalize("M0 0L1", t).is_err());
    }

    #[test]
    fn fill_rules_split_layers() {
        let icon = parse(
            r#"<svg viewBox="0 0 24 24" fill="currentColor"><path fill-rule="evenodd" clip-rule="evenodd" d="M0 0h24v24H0zM6 6h12v12H6z"/><path clip-rule="evenodd" d="M1 1h1v1z"/><path d="M2 2h1v1z"/></svg>"#,
        )
        .unwrap();
        let rules: Vec<_> = icon.layers.iter().map(|l| (l.even_odd, l.path.matches('M').count())).collect();
        assert_eq!(rules, vec![(true, 3), (false, 1)], "{:?}", icon.layers);
        assert!(parse(r#"<svg viewBox="0 0 1 1"><path fill-rule="odd" d="M0 0"/></svg>"#)
            .unwrap_err()
            .contains("evenodd"));
    }

    #[test]
    fn skipped_and_bad_content() {
        let icon = parse(
            r#"<svg viewBox="0 0 2 2"><defs><linearGradient id="g"><stop/></linearGradient></defs><text>Hi</text><rect width="2" height="2" rx="1" fill="url(#g)"/></svg>"#,
        )
        .unwrap();
        assert_eq!(icon.layers.len(), 1);
        assert!(icon.layers[0].path.starts_with("M1 0L1 0A1 1 0 0 1 2 1"), "{}", icon.layers[0].path);
        assert!(parse("<svg><path d=\"M0 0\"/></svg>").unwrap_err().contains("viewBox"));
        assert!(parse("<g/>").unwrap_err().contains("outside <svg>"));
        assert!(parse(r#"<svg viewBox="0 0 1 1"><g transform="rotate(45)"/></svg>"#).unwrap_err().contains("rotate"));
    }
}
