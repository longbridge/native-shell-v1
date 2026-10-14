#![allow(dead_code)] // each example binary uses its own part

//! What the example binaries share: the repository's example programs and the
//! host app pieces some of them need.

pub mod host_demo;

use std::path::PathBuf;

/// One example in the launcher.
pub struct Example {
    pub name: &'static str,
    pub title: &'static str,
    pub summary: &'static str,
    /// A symbolic icon of the Adwaita theme.
    pub icon: &'static str,
    /// The tile's color, `#rrggbb`.
    pub tint: &'static str,
}

pub const SECTIONS: [(&str, &[Example]); 2] = [
    (
        "Apps",
        &[
            Example { name: "tasks", title: "Tasks", summary: "Lists, editing and undo", icon: "object-select-symbolic", tint: "#007aff" },
            Example { name: "chat", title: "Chat", summary: "A thread and its composer", icon: "chat-message-new-symbolic", tint: "#34c759" },
            Example { name: "notes", title: "Notes", summary: "Folders, Markdown and search", icon: "accessories-text-editor-symbolic", tint: "#ffcc00" },
            Example { name: "watchlist", title: "Watchlist", summary: "Host modules and a native cell", icon: "power-profile-performance-symbolic", tint: "#5856d6" },
            Example { name: "settings", title: "Settings", summary: "Grouped preferences and pages", icon: "preferences-system-symbolic", tint: "#8e8e93" },
        ],
    ),
    (
        "Reference",
        &[
            Example { name: "components", title: "Components", summary: "Every control, in context", icon: "view-app-grid-symbolic", tint: "#ff9500" },
            Example { name: "gpui-todolist", title: "gpui-shell to-do list", summary: "A desktop program, unchanged", icon: "computer-symbolic", tint: "#30b0c7" },
        ],
    ),
];

pub fn examples() -> impl Iterator<Item = &'static Example> {
    SECTIONS.iter().flat_map(|(_, examples)| examples.iter())
}

/// The repository's `examples/` directory.
pub fn examples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

/// The program's id from its manifest, for its data directory.
pub fn app_id(dir: &std::path::Path) -> String {
    let manifest = std::fs::read_to_string(dir.join("native-shell.json")).unwrap_or_default();
    serde_json::from_str::<serde_json::Value>(&manifest)
        .ok()
        .and_then(|m| m["id"].as_str().map(String::from))
        .unwrap_or_else(|| dir.file_name().map_or("example".into(), |n| n.to_string_lossy().into_owned()))
}
