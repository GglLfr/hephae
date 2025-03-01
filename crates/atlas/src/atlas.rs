//! Provides texture atlas functionality.
//!
//! A texture atlas contains atlas pages, i.e. lists of textures packed into one large texture in
//! order to reduce the amount of bind groups necessary to hold the information passed to shaders.
//! This means integrating a texture atlas into `Vertex` rendering will significantly increase
//! batching potential, leading to fewer GPU render calls.
//!
//! This module provides the [`TextureAtlas`] type. See [this module](crate::asset) for more
//! information on how the atlas implements [`Asset`].
//!
//! This module provides [`AtlasEntry`] and [`AtlasIndex`] components; the former being the
//! atlas lookup key, and the latter being the cached sprite index. The dedicated
//! [`update_atlas_index`] system listens to changes/additions to texture atlas assets and updates
//! the `AtlasIndex` of entities accordingly.
//!
//! See the `examples/atlas` for a full example.

use std::borrow::Cow;

use bevy_asset::{ReflectAsset, prelude::*};
use bevy_ecs::prelude::*;
use bevy_image::prelude::*;
use bevy_math::prelude::*;
use bevy_reflect::prelude::*;
use bevy_utils::{HashMap, HashSet, prelude::*};
use nonmax::NonMaxUsize;

/// A list of textures packed into one large texture. See the [module-level](crate::atlas)
/// documentation for more specific information on how to integrate this into your rendering
/// framework.
#[derive(Asset, Reflect, Clone)]
#[reflect(Asset)]
pub struct TextureAtlas {
    /// The list of pages contained in this atlas. Items may be modified, but growing or shrinking
    /// this vector is **discouraged**.
    pub pages: Vec<AtlasPage>,
    /// Mapping of sprite names to `(P, Q)` where `P` is the [page index](Self::pages) and `Q` is
    /// the [sprite index](AtlasPage::sprites). Only ever modify if you know what you're doing.
    pub sprite_map: HashMap<String, (usize, usize)>,
}

/// A page located in a [`TextureAtlas`]. Contains the handle to the page image, and rectangle
/// placements of each sprites.
#[derive(Reflect, Clone)]
pub struct AtlasPage {
    /// The page handle.
    pub image: Handle<Image>,
    /// List of sprite rectangle placements in the page; may be looked up from
    /// [`TextureAtlas::sprite_map`].
    pub sprites: Vec<URect>,
}

/// Component denoting a texture atlas sprite lookup key. See the [module-level](crate::atlas)
/// documentation for more specific information on how to integrate this into your rendering
/// framework.
#[derive(Reflect, Component, Clone)]
#[reflect(Component)]
#[require(AtlasIndex)]
pub struct AtlasEntry {
    /// The handle to the texture atlas.
    pub atlas: Handle<TextureAtlas>,
    /// The lookup key.
    pub key: Cow<'static, str>,
}

/// Component denoting a texture atlas cached sprite index. See the [module-level](crate::atlas)
/// documentation for more specific information on how to integrate this into your rendering
/// framework.
#[derive(Component, Default, Copy, Clone)]
pub struct AtlasIndex {
    page_index: Option<NonMaxUsize>,
    sprite_index: Option<NonMaxUsize>,
}

impl AtlasIndex {
    /// Obtains the [page index](TextureAtlas::pages) and [sprite index](AtlasPage::sprites), or
    /// [`None`] if the [key](AtlasIndex) is invalid.
    #[inline]
    pub const fn indices(self) -> Option<(usize, usize)> {
        match (self.page_index, self.sprite_index) {
            (Some(page), Some(sprite)) => Some((page.get(), sprite.get())),
            _ => None,
        }
    }
}

/// System to update [`AtlasIndex`] according to changes [`AtlasEntry`] and [`TextureAtlas`] assets.
pub fn update_atlas_index(
    mut events: EventReader<AssetEvent<TextureAtlas>>,
    atlases: Res<Assets<TextureAtlas>>,
    mut entries: ParamSet<(
        Query<(&AtlasEntry, &mut AtlasIndex), Or<(Changed<AtlasEntry>, Added<AtlasIndex>)>>,
        Query<(&AtlasEntry, &mut AtlasIndex)>,
    )>,
    mut changed: Local<HashSet<AssetId<TextureAtlas>>>,
) {
    changed.clear();
    for &event in events.read() {
        if let AssetEvent::Added { id } | AssetEvent::Modified { id } = event {
            changed.insert(id);
        }
    }

    let update = |entry: &AtlasEntry, mut index: Mut<AtlasIndex>| {
        let Some(atlas) = atlases.get(&entry.atlas) else {
            return;
        };
        let Some(&(page, sprite)) = atlas.sprite_map.get(&*entry.key) else {
            *index = default();
            return;
        };

        *index = AtlasIndex {
            page_index: NonMaxUsize::new(page),
            sprite_index: NonMaxUsize::new(sprite),
        };
    };

    if changed.is_empty() {
        for (entry, index) in &mut entries.p0() {
            update(entry, index);
        }
    } else {
        for (entry, mut index) in &mut entries.p1() {
            if !changed.contains(&entry.atlas.id()) {
                *index = default();
                continue;
            }

            update(entry, index);
        }
    }
}
