#![allow(internal_features)]
#![cfg_attr(any(docsrs, docsrs_dep), feature(rustdoc_internals))]
#![doc = include_str!("../README.md")]
#![cfg_attr(doc, deny(missing_docs))]

#[cfg(feature = "atlas")]
pub use hephae_atlas as atlas;
#[cfg(feature = "locale")]
pub use hephae_locale as locale;
pub use hephae_render as render;
#[cfg(feature = "text")]
pub use hephae_text as text;
#[cfg(feature = "ui")]
pub use hephae_ui as ui;
pub use hephae_utils as utils;

/// Common imports for [`hephae`](crate).
pub mod prelude {
    #[cfg(feature = "atlas")]
    pub use crate::atlas::prelude::*;
    #[cfg(feature = "locale")]
    pub use crate::locale::prelude::*;
    #[cfg(feature = "text")]
    pub use crate::text::prelude::*;
    #[cfg(feature = "ui")]
    pub use crate::ui::prelude::*;
    pub use crate::{render::prelude::*, utils::prelude::*};
}

#[cfg(feature = "atlas")]
pub use crate::atlas::plugin::*;
#[cfg(feature = "locale")]
pub use crate::locale::plugin::*;
pub use crate::render::plugin::*;
#[cfg(feature = "text")]
pub use crate::text::plugin::*;
#[cfg(feature = "ui")]
pub use crate::ui::plugin::*;
