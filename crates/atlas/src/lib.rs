#![doc = include_str!("../README.md")]
#![cfg_attr(doc, deny(missing_docs))]

pub mod asset;
pub mod atlas;
pub mod bind_group;

use bevy_app::prelude::*;
use bevy_asset::prelude::*;
use bevy_ecs::prelude::*;
use bevy_render::{prelude::*, Render, RenderApp};
use hephae_render::prelude::*;

use crate::{
    asset::TextureAtlasLoader,
    atlas::{update_atlas_index, AtlasEntry, AtlasPage, TextureAtlas},
    bind_group::{extract_image_events, validate_image_bind_groups, ImageAssetEvents, ImageBindGroups},
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

        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app
                .init_resource::<ImageAssetEvents>()
                .init_resource::<ImageBindGroups>()
                .add_systems(ExtractSchedule, extract_image_events)
                .add_systems(
                    Render,
                    validate_image_bind_groups.before_ignore_deferred(HephaeRenderSystems::PrepareBindGroups),
                );
        }
    }
}
