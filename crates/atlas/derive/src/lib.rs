#![allow(internal_features)]
#![cfg_attr(any(docsrs, docsrs_dep), feature(rustdoc_internals, doc_cfg))]
#![doc = include_str!("../README.md")]
#![cfg_attr(doc, deny(missing_docs))]

extern crate proc_macro;

mod atlas_entries;

/// Derives `AtlasEntries`.
#[proc_macro_derive(AtlasEntries, attributes(atlas, entry))]
pub fn derive_vertex_layout(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    atlas_entries::parse(input.into())
        .unwrap_or_else(|e| e.into_compile_error())
        .into()
}
