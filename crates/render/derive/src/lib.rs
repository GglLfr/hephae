#![allow(internal_features)]
#![cfg_attr(any(docsrs, docsrs_dep), feature(rustdoc_internals, doc_cfg))]
#![doc = include_str!("../README.md")]
#![cfg_attr(doc, deny(missing_docs))]

extern crate proc_macro;

mod vertex_layout;

/// Derives `VertexLayout`. Note that this also requires `Pod`, which you can derive with
/// `hephae-render`'s re-export: `#[bytemuck(crate = "hephae::render::bytemuck")]`.
#[proc_macro_derive(VertexLayout, attributes(attrib))]
pub fn derive_vertex_layout(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    vertex_layout::parse(input.into())
        .unwrap_or_else(|e| e.into_compile_error())
        .into()
}
