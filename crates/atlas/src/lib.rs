#![allow(internal_features)]
#![cfg_attr(any(docsrs, docsrs_dep), feature(rustdoc_internals))]
#![doc = include_str!("../README.md")]
#![cfg_attr(doc, deny(missing_docs))]

use bevy::prelude::*;
use hephae_utils::prelude::*;

use crate::{
    asset::AtlasLoader,
    atlas::{Atlas, AtlasEntry, AtlasPage, update_atlas_index},
};

pub mod asset;
pub mod atlas;

/// Common imports for [`hephae_atlas`](crate).
pub mod prelude {
    pub use crate::atlas::{Atlas, AtlasEntry, AtlasIndex, AtlasPage};
}

plugin_def! {
    /// Provides texture atlas functionality into the app.
    pub struct AtlasPlugin;
    fn build(&self, app: &mut App) {
        app.init_asset::<Atlas>()
            .register_asset_reflect::<Atlas>()
            .register_asset_loader(AtlasLoader)
            .register_type::<AtlasPage>()
            .register_type::<AtlasEntry>()
            .add_systems(PostUpdate, update_atlas_index);
    }
}
