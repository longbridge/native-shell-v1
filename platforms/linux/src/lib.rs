//! GTK 4 renderer of native-shell for Linux desktops.
//!
//! The core runs the program on its JS thread and hands over binary mutation
//! batches; this crate applies them to a retained GTK widget tree on the main
//! thread, measures text with Pango (the engine that also draws it), and sends
//! input back as events. Every node is a [`NodeView`]: a widget that places its
//! children at the frames taffy computed and paints the box decorations
//! (background, border, corner radii, shadow). Native GTK controls (entries,
//! switches, scrolled windows, drop-downs…) live inside a node as its content.

mod component;
mod components;
mod instance;
mod mount;
mod node_view;
mod surface;
mod text;
mod theme;

pub use instance::{Instance, InstanceOptions};
pub use node_view::NodeView;
pub use surface::SurfaceView;
