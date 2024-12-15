#![doc = include_str!("../README.md")]
#![cfg_attr(doc, deny(missing_docs))]
#![allow(clippy::type_complexity)]
#![allow(clippy::too_many_arguments)]

pub use hephae_render as render;

pub mod prelude {
    pub use crate::render::prelude::*;
}
