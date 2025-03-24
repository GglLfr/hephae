#![allow(internal_features)]
#![cfg_attr(any(docsrs, docsrs_dep), feature(rustdoc_internals))]
#![doc = include_str!("../README.md")]
#![cfg_attr(doc, deny(missing_docs))]

pub use hephae_utils_derive as derive;

mod query_ext;

pub use query_ext::*;

/// Common imports for [`hephae_utils`](crate).
pub mod prelude {
    pub use crate::{
        ComponentOption as _,
        derive::{plugin_conf, plugin_def},
    };
}
