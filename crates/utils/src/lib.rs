#![allow(internal_features)]
#![cfg_attr(docsrs, feature(rustdoc_internals))]
#![doc = include_str!("../README.md")]
#![cfg_attr(doc, deny(missing_docs))]

pub use hephae_utils_derive as derive;

pub mod prelude {
    pub use crate::derive::plugin_conf;
}
