#![doc = include_str!("../README.md")]
#![cfg_attr(doc, deny(missing_docs))]

#[cfg(feature = "atlas")]
pub use hephae_atlas as atlas;
#[cfg(feature = "gui")]
pub use hephae_gui as gui;
pub use hephae_render as render;
#[cfg(feature = "text")]
pub use hephae_text as text;

/// Common imports for [`hephae`](crate).
pub mod prelude {
    #[cfg(feature = "atlas")]
    pub use crate::atlas::prelude::*;
    #[cfg(feature = "gui")]
    pub use crate::gui::prelude::*;
    pub use crate::render::prelude::*;
}
