#![allow(internal_features)]
#![cfg_attr(any(docsrs, docsrs_dep), feature(rustdoc_internals))]
#![doc = include_str!("../README.md")]
#![cfg_attr(doc, deny(missing_docs))]

use bevy::{image::TextureAtlasPlugin, prelude::*};
use hephae_utils::prelude::*;

use crate::{
    atlas::{Atlas, update_atlas_cache},
    loader::AtlasLoader,
};

pub mod atlas;
pub mod loader;

/// Common imports for [`hephae_atlas`](crate).
pub mod prelude {
    pub use crate::atlas::{Atlas, AtlasCache, AtlasEntry, AtlasSprite, NineSliceCuts};
}

plugin_def! {
    /// Provides texture atlas-loading functionality into the app, working in tandem with Bevy's
    /// texture atlas asset.
    pub struct AtlasPlugin;
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<TextureAtlasPlugin>() {
            app.add_plugins(TextureAtlasPlugin);
        }

        app.init_asset::<Atlas>()
            .register_asset_reflect::<Atlas>()
            .register_asset_loader(AtlasLoader)
            .add_systems(PostUpdate, update_atlas_cache.in_set(HephaeAtlasCacheSystem));
    }
}

/// Labels assigned to [`update_atlas_cache`].
#[derive(SystemSet, Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct HephaeAtlasCacheSystem;
