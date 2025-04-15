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
//! This module provides [`AtlasEntry`] and [`AtlasInfo`] components; the former being the
//! atlas lookup key, and the latter being the cached sprite index. The dedicated
//! [`update_atlas_cache`] system listens to changes/additions to texture atlas assets and updates
//! the `AtlasCache` of entities accordingly.
//!
//! See the `examples/atlas` for a full example.

use std::{
    hash::Hash,
    path::{Path, PathBuf},
};

use bevy::{
    asset::ReflectAsset,
    ecs::{component::Tick, system::SystemChangeTick},
    platform::collections::{Equivalent, HashMap},
    prelude::*,
};
use derive_more::{Display, Error, From};
pub use hephae_atlas_derive::AtlasEntries;
use smallvec::SmallVec;

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

#[derive(Error, From, Debug, Display)]
pub enum AtlasEntryError {
    #[display("The given path doesn't resolve to an `AtlasSprite`")]
    SpriteNotFound,
    #[display(
        "The sprite's `TextureAtlasLayout` page doesn't exist or doesn't contain the \
        appropriate the texture; was it modified manually?"
    )]
    LayoutNotFound,
}

impl Atlas {
    /// Gets the appropriate information for rendering the given sprite entry.
    #[inline]
    pub fn get_info(
        &self,
        layouts: &Assets<TextureAtlasLayout>,
        path: &(impl Hash + Equivalent<PathBuf> + ?Sized),
    ) -> Result<AtlasInfo, AtlasEntryError> {
        let entry = self.sprites.get(path).ok_or(AtlasEntryError::SpriteNotFound)?;
        let layout = layouts.get(&entry.atlas.layout).ok_or(AtlasEntryError::LayoutNotFound)?;
        let &rect = layout
            .textures
            .get(entry.atlas.index)
            .ok_or(AtlasEntryError::LayoutNotFound)?;

        Ok(AtlasInfo {
            page: entry.atlas_page.id(),
            page_size: layout.size,
            rect,
            nine_slices: entry.nine_slices,
        })
    }
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

/// Components that want to do an [`Atlas`] [lookup](Atlas::get_info) should derive this trait.
pub trait AtlasEntries: Component {
    /// Returns all the entries to be cached by [`AtlasCaches`], with the same order.
    ///
    /// By default, the derive-macro orders the entries by field declarations.
    fn entries(&self) -> impl Iterator<Item = (AssetId<Atlas>, &Path)>;
}

/// Caches information provided by [`Atlas::get_info`].
#[derive(Copy, Clone, Debug)]
pub struct AtlasInfo {
    /// The page image [`AssetId`], useful for vertex pipeline keys.
    pub page: AssetId<Image>,
    /// The page size, so you don't have to query the image size again.
    pub page_size: UVec2,
    /// Where the sprite resides within the page. Note that images are "+Y = down" so you might have
    /// to flip the V coordinate.
    pub rect: URect,
    /// The nine-slicing information, if any. Useful for tiling and UI.
    pub nine_slices: Option<NineSliceCuts>,
}

impl AtlasInfo {
    /// The default-value for the cache.
    pub const DEFAULT: Self = Self {
        page: AssetId::Uuid {
            uuid: AssetId::<Image>::DEFAULT_UUID,
        },
        page_size: UVec2::ZERO,
        rect: URect {
            min: UVec2::ZERO,
            max: UVec2::ZERO,
        },
        nine_slices: None,
    };
}

impl Default for AtlasInfo {
    #[inline]
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Caches information provided by [`AtlasEntries`].
#[derive(Component, Clone, Debug, Default, Deref, DerefMut)]
pub struct AtlasCaches {
    /// Stores information retrieved through [`Atlas::get_info`]. The element order is defined by
    /// the iterator returned from [`AtlasEntries::entries`].
    pub caches: SmallVec<[AtlasInfo; 4]>,
}

/// Convenience component used to fetch sprite meta information from the given atlas and path.
#[derive(Component, Reflect, AtlasEntries, Clone, Debug)]
#[reflect(Debug)]
pub struct AtlasEntry {
    /// Handle to the [`Atlas`].
    #[atlas]
    pub atlas: Handle<Atlas>,
    /// Sprite full path, including its directories but not its file extension.
    #[entry]
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

#[derive(Resource, Default)]
pub(crate) struct AtlasChanges {
    changes: HashMap<AssetId<Atlas>, Tick>,
    last_change: Tick,
}

pub(crate) fn detect_changes(
    atlases: Res<Assets<Atlas>>,
    mut atlas_events: EventReader<AssetEvent<Atlas>>,
    mut layout_events: EventReader<AssetEvent<TextureAtlasLayout>>,
    mut changes: ResMut<AtlasChanges>,
    tick: SystemChangeTick,
    mut used_layouts: Local<HashMap<AssetId<TextureAtlasLayout>, AssetId<Atlas>>>,
) {
    let tick = tick.this_run();
    let mut has_changes = false;

    for &e in layout_events.read() {
        match e {
            AssetEvent::Added { id } | AssetEvent::Modified { id } | AssetEvent::LoadedWithDependencies { id } => {
                let Some(&atlas_id) = used_layouts.get(&id) else { continue };

                changes.changes.insert(atlas_id, tick);
                has_changes = true
            }
            AssetEvent::Removed { id } | AssetEvent::Unused { id } => {
                if let Some(atlas_id) = used_layouts.remove(&id) {
                    changes.changes.insert(atlas_id, tick);
                    has_changes = true
                }
            }
        }
    }

    for &e in atlas_events.read() {
        match e {
            AssetEvent::Added { id } | AssetEvent::Modified { id } | AssetEvent::LoadedWithDependencies { id } => {
                changes.changes.insert(id, tick);

                let Some(atlas) = atlases.get(id) else { continue };
                for (layout, ..) in &atlas.pages {
                    used_layouts.insert(layout.id(), id);
                    has_changes = true
                }
            }
            AssetEvent::Removed { id } | AssetEvent::Unused { id } => {
                changes.changes.remove(&id);
                has_changes = true
            }
        }
    }

    if has_changes {
        changes.last_change = tick
    }
}

pub(crate) fn update_caches<T: AtlasEntries>(
    atlases: Res<Assets<Atlas>>,
    layouts: Res<Assets<TextureAtlasLayout>>,
    changes: Res<AtlasChanges>,
    tick: SystemChangeTick,
    mut query: Query<(Entity, &T, &mut AtlasCaches)>,
) {
    let last = tick.last_run();
    let this = tick.this_run();

    if !changes.last_change.is_newer_than(last, this) {
        return
    }

    for (e, entries, mut caches) in &mut query {
        let mut len = 0;
        for (i, (atlas, path)) in entries.entries().enumerate() {
            let info = match caches.get_mut(i) {
                Some(info) => info,
                None => {
                    caches.push(AtlasInfo::DEFAULT);
                    &mut caches[i]
                }
            };

            let Some(atlas) = changes
                .changes
                .get(&atlas)
                .and_then(|&tick| tick.is_newer_than(last, this).then_some(atlases.get(atlas)?))
            else {
                *info = AtlasInfo::DEFAULT;
                continue
            };

            match atlas.get_info(&layouts, path) {
                Ok(res) => *info = res,
                Err(error) => {
                    *info = AtlasInfo::DEFAULT;
                    error!("Couldn't update `AtlasCaches` for {e}: {error}")
                }
            }

            len = i;
        }

        caches.truncate(len + 1);
    }
}
