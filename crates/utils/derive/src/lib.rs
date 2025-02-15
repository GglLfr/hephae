#![allow(internal_features)]
#![cfg_attr(any(docsrs, docsrs_dep), feature(rustdoc_internals))]
#![doc = include_str!("../README.md")]
#![cfg_attr(doc, deny(missing_docs))]

extern crate proc_macro;

mod plugin_conf;

/// Generates plugin configuration tuple types for use in plugin group builders.
#[proc_macro]
pub fn plugin_conf(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    plugin_conf::parse(input.into())
        .unwrap_or_else(|e| e.into_compile_error())
        .into()
}
