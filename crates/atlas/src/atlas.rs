//! Provides texture atlas functionality.
//!
//! A texture atlas contains atlas pages, i.e. lists of textures packed into one large texture in
//! order to reduce the amount of bind groups necessary to hold the information passed to shaders.
//! This means integrating a texture atlas into `Vertex` rendering will significantly increase
//! batching potential, leading to fewer GPU render calls.
//!
//! This module provides the [`Atlas`] type. See [this module](crate::atlas) for more
//! information on how the atlas implements [`Asset`].
//!
//! This module provides [`AtlasEntry`] and [`AtlasIndex`] components; the former being the
//! atlas lookup key, and the latter being the cached sprite index. The dedicated
//! [`update_atlas_index`] system listens to changes/additions to texture atlas assets and updates
//! the `AtlasIndex` of entities accordingly.
//!
//! See the `examples/atlas` for a full example.

use std::path::PathBuf;

use bevy::{
    asset::{AsAssetId, ReflectAsset},
    platform_support::collections::HashMap,
    prelude::*,
};

/// A list of textures packed into one large texture. See the [module-level](crate::atlas)
/// documentation for more specific information on how to integrate this into your rendering
/// framework.
#[derive(Asset, Reflect, Clone, Debug)]
#[reflect(Asset, Debug)]
pub struct Atlas {
    /// The atlas page and its sprite layout sizes.
    pub pages: Vec<(Handle<TextureAtlasLayout>, Handle<Image>)>,
    /// Maps sprite paths to their corresponding atlas page and layout.
    pub sprites: HashMap<PathBuf, AtlasSprite>,
}

/// An individual sprite found in an [`Atlas`].
#[derive(Reflect, Clone, Debug)]
#[reflect(Debug)]
pub struct AtlasSprite {
    /// The texture atlas layout, works in tandem `bevy::sprite`.
    pub atlas: TextureAtlas,
    /// The texture atlas image handle, works in tandem with `bevy::sprite`.
    pub atlas_page: Handle<Image>,
    /// Optional nine-slice cuts for this sprite.
    pub nine_slices: Option<NineSliceCuts>,
}

/// Defines horizontal and vertical slashes that split a sprite into nine patches.
#[derive(Reflect, Copy, Clone, Debug)]
#[reflect(Debug)]
pub struct NineSliceCuts {
    /// The leftmost vertical cut. Pixels that `x < left` are considered the left side edge.
    pub left: u32,
    /// The rightmost vertical cut. Pixels that `x > right` are considered the right side edge.
    pub right: u32,
    /// The topmost vertical cut. Pixels that `y < top` are considered the top side edge.
    pub top: u32,
    /// The bottommost vertical cut. Pixels that `y > bottom` are considered the bottom side edge.
    pub bottom: u32,
}

/// Convenience component used to fetch sprite meta information from the given atlas and path.
#[derive(Component, Reflect, Clone, Debug)]
#[require(AtlasCache)]
#[reflect(Debug)]
pub struct AtlasEntry {
    /// Handle to the [`Atlas`].
    pub atlas: Handle<Atlas>,
    /// Sprite full path, including its directories but not its file extension.
    pub path: PathBuf,
}

impl AtlasEntry {
    /// Creates a new [`AtlasEntry`].
    #[inline]
    pub fn new(atlas: Handle<Atlas>, path: impl Into<PathBuf>) -> Self {
        Self {
            atlas,
            path: path.into(),
        }
    }
}

impl AsAssetId for AtlasEntry {
    type Asset = Atlas;

    #[inline]
    fn as_asset_id(&self) -> AssetId<Self::Asset> {
        self.atlas.id()
    }
}

/// Cached information provided by [`AtlasEntry`].
#[derive(Component, Copy, Clone, Debug, Default)]
pub enum AtlasCache {
    /// No cache information is found.
    ///
    /// This typically occurs for any of the following reasons:
    /// - [`update_atlas_cache`] in [`PostUpdate`] hasn't been reached yet.
    /// - The [`Atlas`] doesn't exist.
    /// - The sprite from the given path doesn't exist.
    /// - The sprite in the [`Atlas`] is invalid; i.e., it refers to a non-existent
    ///   [`TextureAtlasLayout`] or to an invalid texture index within that layout. This only
    ///   happens if users manually modify [`Atlas`] assets in an erroneous way.
    #[default]
    None,
    /// Sprite information is successfully cached, ready to be consumed by renderers.
    Cache {
        /// The atlas page ID.
        page: AssetId<Image>,
        /// How large the atlas page is.
        page_size: UVec2,
        /// Where the sprite resides within the atlas, used for UV coordinates.
        rect: URect,
        /// Optional nine-slice cuts associated with the sprite.
        nine_slices: Option<NineSliceCuts>,
    },
}

/// Updates [`AtlasCache`] from [`AtlasEntry`].
pub fn update_atlas_cache(
    atlases: Res<Assets<Atlas>>,
    layouts: Res<Assets<TextureAtlasLayout>>,
    mut entries: Query<(&AtlasEntry, &mut AtlasCache), Or<(Changed<AtlasEntry>, AssetChanged<AtlasEntry>)>>,
) {
    for (entry, mut cache) in &mut entries {
        *cache.bypass_change_detection() = AtlasCache::None;

        let Some(atlas) = atlases.get(&entry.atlas) else {
            error!("Non-existent `Atlas`: {:?}", entry.atlas);
            continue
        };

        let Some(entry) = atlas.sprites.get(&entry.path) else {
            error!("Non-existent sprite path: {}", entry.path.display());
            continue
        };

        let Some(layout) = layouts.get(&entry.atlas.layout) else {
            error!("Non-existent `TextureAtlasLayout`: {:?}", entry.atlas.layout);
            continue
        };

        let Some(&rect) = layout.textures.get(entry.atlas.index) else {
            error!("Non-existent layout index in {:?}: {}", entry.atlas.layout, entry.atlas.index);
            continue
        };

        *cache = AtlasCache::Cache {
            page: entry.atlas_page.id(),
            page_size: layout.size,
            rect,
            nine_slices: entry.nine_slices,
        };
    }
}
