#![allow(internal_features)]
#![cfg_attr(any(docsrs, docsrs_dep), feature(rustdoc_internals))]
#![doc = include_str!("../README.md")]
#![cfg_attr(doc, deny(missing_docs))]

use std::{path::PathBuf, sync::OnceLock};

pub use proc_macro2;
pub use quote;
use quote::ToTokens;
pub use syn;
use syn::Path;
use toml_edit::{DocumentMut, Item};

/// Represents `Cargo.toml`, providing functions to resolve library paths under Bevy or similar
/// library-containing crates.
pub struct Manifest(DocumentMut);
impl Manifest {
    fn new() -> Self {
        Self(
            std::env::var_os("CARGO_MANIFEST_DIR")
                .map(PathBuf::from)
                .map(|mut path| {
                    path.push("Cargo.toml");
                    if !path.exists() {
                        panic!("No Cargo manifest found for crate. Expected: {}", path.display());
                    }

                    std::fs::read_to_string(path.clone())
                        .unwrap_or_else(|e| panic!("Unable to read cargo manifest ({}): {e}", path.display()))
                        .parse::<DocumentMut>()
                        .unwrap_or_else(|e| panic!("Failed to parse cargo manifest ({}): {e}", path.display()))
                })
                .expect("CARGO_MANIFEST_DIR is not defined."),
        )
    }

    /// Gets a lazily-initialized static instance of [`Manifest`].
    #[inline]
    pub fn get() -> &'static Self {
        static INSTANCE: OnceLock<Manifest> = OnceLock::new();
        INSTANCE.get_or_init(Self::new)
    }

    /// Resolves `bevy::{sub}`.
    #[inline]
    pub fn resolve_bevy(sub: impl AsRef<str>, tokens: impl ToTokens) -> syn::Result<Path> {
        Self::get().resolve("bevy", sub, tokens)
    }

    /// Resolves `hephae::{sub}`.
    #[inline]
    pub fn resolve_hephae(sub: impl AsRef<str>, tokens: impl ToTokens) -> syn::Result<Path> {
        Self::get().resolve("hephae", sub, tokens)
    }

    /// Resolves a sub-crate under the base crate, i.e., `render` under `bevy`.
    pub fn resolve(&self, base: impl AsRef<str>, sub: impl AsRef<str>, tokens: impl ToTokens) -> syn::Result<Path> {
        let name = |dep: &Item, name: &str| -> String {
            if dep.as_str().is_some() {
                name.into()
            } else {
                dep.get("package").and_then(|name| name.as_str()).unwrap_or(name).into()
            }
        };

        let base = base.as_ref();
        let sub = sub.as_ref();

        let find = |deps: &Item| -> Option<syn::Result<syn::Path>> {
            if let Some(dep) = deps.get(format!("{base}_{sub}")) {
                Some(syn::parse_str(&format!("{}_{sub}", name(dep, base))))
            } else if let Some(dep) = deps.get(format!("{base}-{sub}")) {
                Some(syn::parse_str(&format!("{}_{sub}", name(dep, base))))
            } else { deps.get(base).map(|dep| syn::parse_str(&format!("{}::{sub}", name(dep, base)))) }
        };

        match self
            .0
            .get("dependencies")
            .and_then(find)
            .or_else(|| self.0.get("dev-dependencies").and_then(find))
            .ok_or_else(|| syn::Error::new_spanned(&tokens, format!("Missing dependency `{base}::{sub}`")))
        {
            Ok(Ok(path)) => Ok(path),
            Ok(Err(error)) | Err(error) => Err(error),
        }
    }
}
