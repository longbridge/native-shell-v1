//! Text measurement with Pango, on the JS thread.

use native_shell_core::host::{TextMetrics, WidthMode};

pub fn measure(_attributed_json: &str, _width: WidthMode) -> TextMetrics {
    TextMetrics::default()
}
