//! Loading a signed bundle, with and without bytecode.

use std::collections::BTreeMap;
use std::sync::Arc;

use native_shell::core::manifest::Manifest;
use native_shell::testing::FakeHost;
use native_shell::{Instance, InstanceConfig};
use native_shell_services::bundle::{self, BuildOptions, TrustedKey};

const MAIN: &str = r#"
import { View, div } from "native-shell";
import { greeting } from "./lib/greet.js";
export default class B extends View { render() { return div().test_id("g").child(greeting("bundle")); } }
"#;

fn load(bytecode: bool) -> Instance {
    let secret = [7u8; 32];
    let manifest = Manifest::parse(r#"{"id":"t.bundle","name":"Bundle","version":"1.0.0"}"#).unwrap();
    let files = BTreeMap::from([
        ("main.js".to_string(), MAIN.as_bytes().to_vec()),
        ("lib/greet.js".to_string(), b"export const greeting = (who) => `hello ${who}`;".to_vec()),
    ]);
    let opts = BuildOptions { bytecode, ..BuildOptions::default() };
    let bytes = bundle::build(&files, &manifest, &secret, "k1", &opts).unwrap();
    let trusted = [TrustedKey { id: "k1".into(), public_key: bundle::public_key_of(&secret) }];
    let verified = bundle::open(&bytes, &trusted, &[], env!("CARGO_PKG_VERSION")).unwrap();
    assert_eq!(!verified.bytecode.is_empty(), bytecode);
    let mut instance = Instance::new(InstanceConfig::new("t.bundle"), Arc::new(FakeHost::new())).unwrap();
    instance.load_bundle(verified).unwrap();
    instance
}

#[test]
fn bundles_load_from_sources_and_from_bytecode() {
    for bytecode in [false, true] {
        let instance = load(bytecode);
        let node = instance.tree().find_by_test_id("g").expect("rendered");
        assert!(
            instance.tree().dump().contains("hello bundle"),
            "bytecode={bytecode} node={node}\n{}",
            instance.tree().dump()
        );
    }
}
