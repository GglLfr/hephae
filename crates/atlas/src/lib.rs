#![allow(internal_features)]
#![cfg_attr(docsrs, feature(rustdoc_internals))]
#![doc = include_str!("../README.md")]
#![cfg_attr(doc, deny(missing_docs))]

pub mod asset;
pub mod atlas;

/// Common imports for [`hephae_atlas`](crate).
pub mod prelude {
    pub use crate::atlas::{AtlasEntry, AtlasIndex, AtlasPage, TextureAtlas};
}

/// App plugins for [`hephae_atlas`](crate).
pub mod plugin {
    use bevy_app::prelude::*;
    use bevy_asset::prelude::*;

    use crate::{
        asset::TextureAtlasLoader,
        atlas::{update_atlas_index, AtlasEntry, AtlasPage, TextureAtlas},
    };

    /// Provides texture atlas functionality into the app.
    pub fn atlas() -> impl Plugin {
        |app: &mut App| {
            app.init_asset::<TextureAtlas>()
                .register_asset_reflect::<TextureAtlas>()
                .register_asset_loader(TextureAtlasLoader)
                .register_type::<AtlasPage>()
                .register_type::<AtlasEntry>()
                .add_systems(PostUpdate, update_atlas_index);
        }
    }
}
