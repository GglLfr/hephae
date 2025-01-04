#![doc = include_str!("../README.md")]
#![cfg_attr(doc, deny(missing_docs))]

pub mod asset;
pub mod atlas;

use bevy_app::prelude::*;
use bevy_asset::prelude::*;

use crate::{
    asset::TextureAtlasLoader,
    atlas::{update_atlas_index, AtlasEntry, AtlasPage, TextureAtlas},
};

/// Common imports for [`hephae_render`](crate).
pub mod prelude {
    pub use crate::{
        atlas::{AtlasEntry, AtlasIndex, AtlasPage, TextureAtlas},
        AtlasPlugin,
    };
}

/// Provides texture atlas functionality. Registers [`TextureAtlas`] and [`TextureAtlasLoader`].
pub struct AtlasPlugin;
impl Plugin for AtlasPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<TextureAtlas>()
            .register_asset_reflect::<TextureAtlas>()
            .register_asset_loader(TextureAtlasLoader)
            .register_type::<AtlasPage>()
            .register_type::<AtlasEntry>()
            .add_systems(PostUpdate, update_atlas_index);
    }
}
