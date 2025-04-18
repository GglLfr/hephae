//! Defines font atlas sets for rendering glyphs.

use bevy::{
    asset::RenderAssetUsages,
    platform::{collections::HashMap, hash::FixedHasher},
    prelude::*,
    render::{
        Extract,
        render_resource::{Extent3d, TextureDimension, TextureFormat},
    },
};
use cosmic_text::{CacheKey, FontSystem, LayoutGlyph, Placement, SwashCache, SwashContent};
use guillotiere::{AtlasAllocator, size2};

use crate::layout::FontLayoutError;

#[derive(Default)]
pub(crate) struct FontAtlases(HashMap<FontAtlasKey, Handle<FontAtlas>>);
impl FontAtlases {
    #[inline]
    pub fn atlas_mut<'a>(
        &mut self,
        key: FontAtlasKey,
        atlases: &'a mut Assets<FontAtlas>,
    ) -> (AssetId<FontAtlas>, &'a mut FontAtlas) {
        let id = self
            .0
            .entry(key)
            .or_insert_with(|| {
                atlases.add(FontAtlas {
                    key,
                    alloc: AtlasAllocator::new(size2(512, 512)),
                    image: Handle::Weak(AssetId::invalid()),
                    map: HashMap::with_hasher(FixedHasher),
                    nodes: Vec::new(),
                })
            })
            .id();

        (id, atlases.get_mut(id).unwrap())
    }
}

#[derive(Copy, Clone, Hash, Eq, PartialEq)]
pub(crate) struct FontAtlasKey {
    pub font_size: u32,
    pub antialias: bool,
}

/// A texture atlas dynamically containing a font's glyphs.
#[derive(Asset, TypePath)]
pub struct FontAtlas {
    key: FontAtlasKey,
    alloc: AtlasAllocator,
    image: Handle<Image>,
    map: HashMap<CacheKey, usize>,
    nodes: Vec<(IVec2, URect)>,
}

impl FontAtlas {
    /// Gets the underlying images of this font atlas.
    #[inline]
    pub fn image(&self) -> AssetId<Image> {
        self.image.id()
    }

    /// Gets the size of this font atlas.
    #[inline]
    pub fn size(&self) -> UVec2 {
        let size = self.alloc.size().cast::<u32>();
        uvec2(size.width, size.height)
    }

    /// Gets the glyph information in a tuple of positional offset, size, and index.
    #[inline]
    pub fn get_info(&self, glyph: &LayoutGlyph) -> Option<(IVec2, URect, usize)> {
        self.map.get(&glyph.physical((0., 0.), 1.).cache_key).and_then(|&index| {
            let (offset, rect) = self.get_info_index(index)?;
            Some((offset, rect, index))
        })
    }

    /// Gets the glyph information based on its index in a tuple of positional offset and size.
    #[inline]
    pub fn get_info_index(&self, index: usize) -> Option<(IVec2, URect)> {
        Some(*self.nodes.get(index)?)
    }

    /// Gets or loads the glyph information.
    pub fn get_or_create_info(
        &mut self,
        sys: &mut FontSystem,
        cache: &mut SwashCache,
        glyph: &LayoutGlyph,
        images: &mut Assets<Image>,
    ) -> Result<(IVec2, URect, usize), FontLayoutError> {
        if let Some(info) = self.get_info(glyph) {
            return Ok(info);
        }

        let phys = glyph.physical((0., 0.), 1.);
        let swash_image = cache
            .get_image_uncached(sys, phys.cache_key)
            .ok_or(FontLayoutError::NoGlyphImage(phys.cache_key))?;

        let Placement {
            left,
            top,
            width,
            height,
        } = swash_image.placement;

        if width == 0 || height == 0 {
            self.map.insert(phys.cache_key, self.nodes.len());
            self.nodes.push((ivec2(left, top), URect::new(0, 0, width, height)));

            Ok((ivec2(left, top), URect::new(0, 0, width, height), self.nodes.len() - 1))
        } else {
            loop {
                match self.alloc.allocate(size2(width as i32 + 2, height as i32 + 2)) {
                    Some(alloc) => {
                        let mut rect = alloc.rectangle.cast::<u32>();
                        rect.min.x += 1;
                        rect.min.y += 1;
                        rect.max.x -= 1;
                        rect.max.y -= 1;

                        let alloc_size = self.alloc.size().cast::<u32>();

                        self.map.insert(phys.cache_key, self.nodes.len());
                        self.nodes
                            .push((ivec2(left, top), URect::new(rect.min.x, rect.min.y, rect.max.x, rect.max.y)));

                        let image = match images.get_mut(&self.image) {
                            Some(image)
                                if {
                                    let size = image.texture_descriptor.size;
                                    size.width == alloc_size.width && size.height == alloc_size.height
                                } =>
                            {
                                image
                            }
                            _ => {
                                let old = images.remove(&self.image);
                                let new = Image::new(
                                    Extent3d {
                                        width: alloc_size.width,
                                        height: alloc_size.height,
                                        depth_or_array_layers: 1,
                                    },
                                    TextureDimension::D2,
                                    match old {
                                        Some(mut old) => {
                                            let old_size = old.texture_descriptor.size;
                                            let mut data = Vec::with_capacity(
                                                alloc_size.width as usize * alloc_size.height as usize * 4,
                                            );

                                            let copy_amount = (old_size.width.min(alloc_size.width) * 4) as usize;
                                            let copy_left =
                                                alloc_size.width.saturating_sub(old_size.width.min(alloc_size.width));

                                            let old_data = old.data.as_mut().unwrap();
                                            for y in 0..old_size.height as usize {
                                                data.extend_from_slice(
                                                    &old_data[(y * copy_amount)..((y + 1) * copy_amount)],
                                                );
                                                for _ in 0..copy_left {
                                                    data.extend_from_slice(&[0, 0, 0, 0]);
                                                }
                                            }

                                            data
                                        }
                                        None => vec![0; alloc_size.width as usize * alloc_size.height as usize * 4],
                                    },
                                    TextureFormat::Rgba8UnormSrgb,
                                    RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
                                );

                                self.image = images.add(new);
                                images.get_mut(&self.image).unwrap()
                            }
                        };

                        let row = image.texture_descriptor.size.width as usize * 4;
                        let from_x = rect.min.x as usize;
                        let to_x = rect.max.x as usize;
                        let src_row = to_x - from_x;

                        let alpha = |a| match self.key.antialias {
                            false => {
                                if a > 127 {
                                    255
                                } else {
                                    0
                                }
                            }
                            true => a,
                        };

                        let data = image.data.as_mut().unwrap();
                        for (src_y, dst_y) in (rect.min.y as usize..rect.max.y as usize).enumerate() {
                            for (src_x, dst_x) in (from_x..to_x).enumerate() {
                                data[dst_y * row + dst_x * 4..dst_y * row + dst_x * 4 + 4].copy_from_slice(
                                    &match swash_image.content {
                                        SwashContent::Mask => {
                                            [255, 255, 255, alpha(swash_image.data[src_y * src_row + src_x])]
                                        }
                                        SwashContent::Color => {
                                            let data = &swash_image.data
                                                [src_y * src_row * 4 + src_x * 4..src_y * src_row * 4 + src_x * 4 + 4];
                                            [data[0], data[1], data[2], alpha(data[3])]
                                        }
                                        SwashContent::SubpixelMask => {
                                            unimplemented!("sub-pixel antialiasing is unimplemented")
                                        }
                                    },
                                );
                            }
                        }

                        break Ok((
                            ivec2(left, top),
                            URect::new(rect.min.x, rect.min.y, rect.max.x, rect.max.y),
                            self.nodes.len() - 1,
                        ));
                    }
                    None => self.alloc.grow(self.alloc.size() * 2),
                }
            }
        }
    }
}

/// Extracted glyph information for use in rendering.
#[derive(Resource, Default)]
pub struct ExtractedFontAtlases(HashMap<AssetId<FontAtlas>, ExtractedFontAtlas>);
impl ExtractedFontAtlases {
    /// Gets a [`ExtractedFontAtlas`] based on its asset ID.
    #[inline]
    pub fn get(&self, id: impl Into<AssetId<FontAtlas>>) -> Option<&ExtractedFontAtlas> {
        self.0.get(&id.into())
    }
}

/// Extracted glyph information for use in rendering.
#[derive(Default)]
pub struct ExtractedFontAtlas {
    image: AssetId<Image>,
    size: UVec2,
    nodes: Vec<(IVec2, URect)>,
}

impl ExtractedFontAtlas {
    /// Gets the underlying images of this font atlas.
    #[inline]
    pub fn image(&self) -> AssetId<Image> {
        self.image
    }

    /// Gets the size of this font atlas.
    #[inline]
    pub fn size(&self) -> UVec2 {
        self.size
    }

    /// Gets the glyph information based on its index in a tuple of positional offset and size.
    #[inline]
    pub fn get_info_index(&self, index: usize) -> Option<(IVec2, URect)> {
        self.nodes.get(index).copied()
    }
}

/// Extracts font atlases into the render world.
pub fn extract_font_atlases(
    mut extracted: ResMut<ExtractedFontAtlases>,
    atlases: Extract<Res<Assets<FontAtlas>>>,
    mut atlas_events: Extract<EventReader<AssetEvent<FontAtlas>>>,
) {
    for (id, atlas) in atlases.iter() {
        let dst = extracted.0.entry(id).or_default();
        dst.image = atlas.image.id();
        dst.size = atlas.size();
        dst.nodes.clear();
        dst.nodes.extend(&atlas.nodes);
    }

    for &e in atlas_events.read() {
        match e {
            AssetEvent::Added { .. } | AssetEvent::Modified { .. } | AssetEvent::LoadedWithDependencies { .. } => {}
            AssetEvent::Removed { id } | AssetEvent::Unused { id } => {
                extracted.0.remove(&id);
            }
        }
    }
}
