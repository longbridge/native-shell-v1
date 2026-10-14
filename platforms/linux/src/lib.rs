//! GTK 4 renderer of native-shell for Linux desktops.
//!
//! The core runs the program on its JS thread and hands over binary mutation
//! batches; this crate applies them to a retained GTK widget tree on the main
//! thread, measures text with Pango (the engine that also draws it), and sends
//! input back as events. Every node is a [`NodeView`]: a widget that places its
//! children at the frames taffy computed and paints the box decorations
//! (background, border, corner radii, shadow). Native GTK controls (entries,
//! switches, scrolled windows, drop-downs…) live inside a node as its content.

mod a11y;
mod animator;
mod clipboard;
mod component;
mod dev;
mod components;
mod gestures;
#[cfg(test)]
mod gtk_test;
mod instance;
mod keys;
mod mount;
mod node_view;
mod root_commands;
mod runtime;
mod surface;
mod text;
mod theme;
mod toast;

pub use dev::DevClient;
pub use instance::{default_data_dir, Instance, InstanceOptions};
pub use native_shell::{Limits, Metrics, ThemeConfig};
pub use native_shell_core::host::{
    Environment, HostComponentDescriptor, HostMethod, HostModuleDescriptor, HostRegistry, LogLevel, MethodKind, WidthMode,
};
pub use native_shell_core::manifest::Capabilities;
pub use native_shell_core::theme::Appearance;
pub use runtime::{CallContext, EventSink, HostComponent, HostComponentType, HostModule, Responder, Runtime};
pub use node_view::NodeView;
pub use surface::SurfaceView;
/// The user's text size factor (desktop text scaling), for host components that size text like the shell.
pub use text::font_scale;
