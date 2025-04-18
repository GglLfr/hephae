#![allow(internal_features)]
#![cfg_attr(any(docsrs, docsrs_dep), feature(rustdoc_internals, doc_cfg))]
#![doc = include_str!("../README.md")]
#![cfg_attr(doc, deny(missing_docs))]

extern crate proc_macro;

use hephae_macros::syn::Error;

mod plugin_conf;
mod plugin_def;

/// Generates plugin/plugin-group structs that automatically derive [`Copy`], [`Clone`],
/// [`Default`], and [`Debug`](core::fmt::Debug) regardless of the generic types.
#[proc_macro]
pub fn plugin_def(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    plugin_def::parse(input.into())
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

/// Generates plugin configuration tuple types for use in plugin group builders.
#[proc_macro]
pub fn plugin_conf(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    plugin_conf::parse(input.into())
        .unwrap_or_else(Error::into_compile_error)
        .into()
}
