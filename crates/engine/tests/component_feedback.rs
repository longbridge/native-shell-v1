//! "gpui-component" feedback components: Alert, Badge, Tag, Skeleton, Empty,
//! Kbd, Label, Separator, ShimmerText and Notification.

use native_shell::testing::Harness;

#[test]
fn gpui_component_still_exports_title_bar_next_to_the_area_modules() {
    let h = Harness::new(&[(
        "main.js",
        r#"
import { View } from "native-shell";
import * as gc from "gpui-component";
import { TitleBar } from "gpui-component";
export default class C extends View {
  render() { return new TitleBar().test_id("bar").child(Object.keys(gc).sort().join(",")); }
}
"#,
    )]);
    assert!(h.load_error().is_none(), "{:?} {:?}", h.load_error(), h.logs());
    assert!(h.find("bar").is_some());
}
