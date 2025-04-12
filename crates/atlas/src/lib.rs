#![allow(internal_features)]
#![cfg_attr(any(docsrs, docsrs_dep), feature(rustdoc_internals))]
#![doc = include_str!("../README.md")]
#![cfg_attr(doc, deny(missing_docs))]

use bevy::{app::PluginGroupBuilder, asset::AssetEvents, image::TextureAtlasPlugin, prelude::*};
use hephae_utils::prelude::*;

use crate::{
    atlas::{Atlas, AtlasCaches, AtlasChanges, AtlasEntries, AtlasEntry, detect_changes, update_caches},
    loader::AtlasLoader,
};

pub mod atlas;
pub mod loader;

/// Common imports for [`hephae_atlas`](crate).
pub mod prelude {
    pub use crate::atlas::{Atlas, AtlasCaches, AtlasEntries, AtlasEntry, AtlasInfo, AtlasSprite, NineSliceCuts};
}

plugin_conf! {
    /// [`AtlasEntries`]s you can pass to [`AtlasPlugin`] to conveniently configure them in one go.
    pub trait EntryConf for AtlasEntries, T => AtlasEntryPlugin::<T>::default()
}

plugin_def! {
    /// Configures additional texture atlas entry lookup components.
    pub struct AtlasEntryPlugin<T: AtlasEntries>;
    fn build(&self, app: &mut App) {
        app.add_systems(PostUpdate, update_caches::<T>.in_set(HephaeAtlasCacheSystem::UpdateCaches))
            .world_mut()
            .register_required_components::<T, AtlasCaches>()
    }
}

plugin_def! {
    /// Provides texture atlas-loading functionality into the app, working in tandem with Bevy's
    /// texture atlas asset.
    #[plugin_group]
    pub struct AtlasPlugin<T: EntryConf = ()>;
    fn build(self) -> PluginGroupBuilder {
        T::build(
            PluginGroupBuilder::start::<Self>()
                .add(|app: &mut App| {
                    if !app.is_plugin_added::<TextureAtlasPlugin>() {
                        app.add_plugins(TextureAtlasPlugin);
                    }

                    app.init_resource::<AtlasChanges>()
                        .init_asset::<Atlas>()
                        .register_asset_reflect::<Atlas>()
                        .register_asset_loader(AtlasLoader)
                        .configure_sets(
                            PostUpdate,
                            (
                                AssetEvents,
                                HephaeAtlasCacheSystem::AssetChanges,
                                HephaeAtlasCacheSystem::UpdateCaches,
                            )
                                .chain(),
                        )
                        .add_systems(
                            PostUpdate,
                            detect_changes.in_set(HephaeAtlasCacheSystem::AssetChanges),
                        );
                })
                .add(AtlasEntryPlugin::<AtlasEntry>::default()),
        )
    }
}

/// Labels assigned to [`update_atlas_cache`].
#[derive(SystemSet, Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum HephaeAtlasCacheSystem {
    /// Detects for changes to [`Atlas`] and layout asset changes.
    AssetChanges,
    /// If an asset change is detected, update the atlas caches.
    UpdateCaches,
}
