//! `svg(path)` and `image(path)` with the program's own files.

use std::collections::BTreeMap;
use std::sync::Arc;

use native_shell::core::manifest::Manifest;
use native_shell::core::protocol::{Prop, PropValue, ViewType};
use native_shell::testing::{FakeHost, Harness};
use native_shell::{Instance, InstanceConfig};
use native_shell_services::bundle::{self, BuildOptions, TrustedKey};

const MAIN: &str = r##"
import { View, div, v_flex, svg, image } from "native-shell";
export default class A extends View {
  render(cx) {
    return v_flex()
      .child(svg("icons/badge.svg").test_id("badge").size_6().text_color("#ff0000"))
      .child(svg("icons/dot.svg").test_id("dot").size_4())
      .child(image("img/logo.png").test_id("logo").size_8())
      .child(image("https://example.com/a.png").test_id("remote").size_8())
      .child(image("img/missing.png").test_id("missing").size_8());
  }
}
"##;

/// A filled disc with a stroked ring: two paints.
const BADGE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><circle cx="8" cy="8" r="3"/><circle cx="8" cy="8" r="6" fill="none" stroke="currentColor" stroke-width="1.5"/></svg>"#;
const DOT: &str = r#"<svg viewBox="0 0 2 2"><rect width="2" height="2" rx="1"/></svg>"#;
/// "PNG" bytes, not valid UTF-8, passed in sources as a data URL.
const LOGO: &[u8] = &[0x89, b'P', b'N', b'G', 0xff, 0x00, 0x01];
const LOGO_B64: &str = "data:image/png;base64,iVBOR/8AAQ==";

fn manifest() -> &'static str {
    r#"{"id":"t.assets","name":"Assets","capabilities":{"network":{"hosts":["example.com"]}}}"#
}

#[test]
fn svgs_become_shapes_and_images_files_on_disk() {
    let dir = std::env::temp_dir().join(format!("nsh-assets-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let files = [("main.js", MAIN), ("icons/badge.svg", BADGE), ("icons/dot.svg", DOT), ("img/logo.png", LOGO_B64)];
    let h = Harness::build(&files).manifest(manifest()).data_dir(&dir).start();
    assert!(h.load_error().is_none(), "{:?}", h.load_error());

    // One paint: the element itself is the shape, filled with the theme foreground.
    assert_eq!(h.view_type("dot"), ViewType::Shape);
    assert_eq!(
        h.prop("dot", Prop::PathData),
        Some(PropValue::Str("M1 0L1 0A1 1 0 0 1 2 1L2 1A1 1 0 0 1 1 2L1 2A1 1 0 0 1 0 1L0 1A1 1 0 0 1 1 0Z".into()))
    );
    assert_eq!(h.prop("dot", Prop::ViewBox), Some(PropValue::F32Array(vec![0.0, 0.0, 2.0, 2.0])));
    let foreground = h.theme_tokens()[0]["foreground"].as_u64().expect("foreground token") as u32;
    assert_eq!(h.prop("dot", Prop::FillColor), Some(PropValue::Color(foreground)));
    assert_eq!(h.prop("dot", Prop::StrokeColor), None);

    // Two paints: a box holding a filled and a stroked shape, both in the text color.
    assert_eq!(h.view_type("badge"), ViewType::View);
    let layers = h.native().children(h.find("badge").unwrap()).to_vec();
    assert_eq!(layers.len(), 2);
    let red = PropValue::Color(0xFFFF_0000);
    assert_eq!(h.native().prop(layers[0], Prop::FillColor), Some(&red));
    assert_eq!(h.native().prop(layers[1], Prop::StrokeColor), Some(&red));
    assert_eq!(h.native().prop(layers[1], Prop::StrokeWidth), Some(&PropValue::F32(1.5)));
    for layer in &layers {
        let f = h.native().nodes[layer].frame.expect("laid out");
        assert_eq!((f.x, f.y, f.width, f.height), (0.0, 0.0, 24.0, 24.0), "each layer covers the icon");
    }

    // A program image is written once under <data_dir>/.assets and sent by absolute path.
    let Some(PropValue::Str(path)) = h.prop("logo", Prop::ImageSource) else { panic!("no image source") };
    assert!(path.starts_with(dir.join(".assets").to_str().unwrap()) && path.ends_with(".png"), "{path}");
    assert_eq!(std::fs::read(&path).unwrap(), LOGO);
    assert_eq!(h.prop("remote", Prop::ImageSource), Some(PropValue::Str("https://example.com/a.png".into())));
    assert_eq!(h.prop("missing", Prop::ImageSource), None);
    let warnings: Vec<_> = h.logs().into_iter().filter(|l| l.contains("nothing is drawn")).collect();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("image(\"img/missing.png\"): no such file in the program"), "{warnings:?}");

    // Same bytes, same file: a second instance reuses it.
    let again = Harness::build(&files).manifest(manifest()).data_dir(&dir).start();
    assert_eq!(again.prop("logo", Prop::ImageSource), Some(PropValue::Str(path.clone())));
    assert_eq!(std::fs::read_dir(dir.join(".assets")).unwrap().count(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn bundle_files_serve_svgs_and_images() {
    let secret = [9u8; 32];
    let manifest = Manifest::parse(manifest()).unwrap();
    let files = BTreeMap::from([
        ("main.js".to_string(), MAIN.as_bytes().to_vec()),
        ("icons/badge.svg".to_string(), BADGE.as_bytes().to_vec()),
        ("icons/dot.svg".to_string(), DOT.as_bytes().to_vec()),
        ("img/logo.png".to_string(), LOGO.to_vec()),
    ]);
    let bytes = bundle::build(&files, &manifest, &secret, "k1", &BuildOptions::default()).unwrap();
    let trusted = [TrustedKey { id: "k1".into(), public_key: bundle::public_key_of(&secret) }];
    let verified = bundle::open(&bytes, &trusted, &[], env!("CARGO_PKG_VERSION")).unwrap();
    // No data directory: the copy goes to the temp directory.
    let mut instance = Instance::new(InstanceConfig::new("t.assets"), Arc::new(FakeHost::new())).unwrap();
    instance.load_bundle(verified).unwrap();
    let dump = instance.tree().dump();
    assert!(dump.lines().any(|l| l.trim_start().starts_with("Shape") && l.contains("test_id=\"dot\"")), "{dump}");
    let tmp = std::env::temp_dir().join("native-shell-assets");
    let copy = std::fs::read_dir(&tmp)
        .unwrap()
        .filter_map(|e| e.ok())
        .find(|e| std::fs::read(e.path()).ok().as_deref() == Some(LOGO));
    assert!(copy.is_some(), "the PNG was written under {}", tmp.display());
}

#[test]
fn even_odd_fills_reach_the_shape() {
    let main = r#"
import { View, v_flex, svg, shape } from "native-shell";
export default class F extends View {
  render() {
    return v_flex()
      .child(svg("ring.svg").test_id("ring").size_6())
      .child(shape("M0 0h4v4H0zM1 1h2v2H1z", { view_box: [0, 0, 4, 4], fill_rule: "evenodd" }).fill("foreground").test_id("hole").size_4())
      .child(shape("M0 0h4v4H0z", { view_box: [0, 0, 4, 4] }).fill("foreground").test_id("plain").size_4());
  }
}
"#;
    let ring = r#"<svg viewBox="0 0 24 24" fill="currentColor"><path fill-rule="evenodd" clip-rule="evenodd" d="M0 0h24v24H0zM6 6h12v12H6z"/></svg>"#;
    let h = Harness::new(&[("main.js", main), ("ring.svg", ring)]);
    assert_eq!(h.prop("ring", Prop::FillRule), Some(PropValue::I32(1)));
    assert_eq!(h.prop("hole", Prop::FillRule), Some(PropValue::I32(1)));
    assert_eq!(h.prop("plain", Prop::FillRule), None, "nonzero is the default and is not sent");
}
