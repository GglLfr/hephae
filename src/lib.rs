#![doc = include_str!("../README.md")]
#![cfg_attr(doc, deny(missing_docs))]

#[cfg(feature = "atlas")]
pub use hephae_atlas as atlas;
pub use hephae_render as render;

/// Common imports for [`hephae`](crate).
pub mod prelude {
    #[cfg(feature = "atlas")]
    pub use crate::atlas::prelude::*;
    pub use crate::render::prelude::*;
}
