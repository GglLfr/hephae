//! [`AssetLoader`] implementation for loading [`Atlas`].
//!
//! Format is as the following example:
//! ```ron
//! (
//!     padding: 4,
//!     bleeding: 4,
//!     usages: (
//!         main: false,
//!         render: true,
//!     ),
//!     entries: [
//!         "some-file-relative-to-atlas-file.png",
//!         ("some-dir-relative-to-atlas-file", [
//!             "some-file-inside-the-dir.png",
//!         ]),
//!     ],
//! )
//! ```

use std::{
    io,
    path::{Path, PathBuf},
};

use bevy::{
    asset::{
        AssetLoader, AssetPath, LoadContext, LoadDirectError, ParseAssetPathError, RenderAssetUsages, io::Reader, ron,
        ron::error::SpannedError,
    },
    image::TextureFormatPixelInfo,
    platform_support::{collections::HashMap, hash::FixedHasher},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use derive_more::{Display, Error, From};
use guillotiere::{
    AllocId, AtlasAllocator, Change, ChangeList,
    euclid::{Box2D, Size2D},
    size2,
};
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{Error as DeError, MapAccess, Visitor},
    ser::SerializeStruct,
};

use crate::atlas::{Atlas, AtlasSprite, NineSliceCuts};

/// Asset file representation of [`Atlas`].
///
/// This struct `impl`s [`Serialize`] and
/// [`Deserialize`], which means it may be (de)serialized into any implementation, albeit
/// [`AtlasLoader`] uses [RON](ron) format specifically.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AtlasFile {
    /// How far away the edges of one sprite to another and to the page boundaries, in pixels. This
    /// may be utilized to mitigate the imperfect precision with texture sampling where a fraction
    /// of neighboring sprites actually get sampled instead.
    #[serde(default = "AtlasFile::default_padding")]
    pub padding: u32,
    /// How much the sprites will "bleed" outside its edge. That is, how much times the edges of a
    /// sprite is copied to its surrounding border, creating a bleeding effect. This may be utilized
    /// to mitigate the imperfect precision with texture sampling where the edge of a sprite doesn't
    /// quite reach the edge of the vertices.
    #[serde(default = "AtlasFile::default_bleeding")]
    pub bleeding: u32,
    #[serde(
        default = "AtlasFile::default_usages",
        serialize_with = "AtlasFile::serialize_usages",
        deserialize_with = "AtlasFile::deserialize_usages"
    )]
    /// Defines the usages for the resulting atlas pages.
    pub usages: RenderAssetUsages,
    /// File entries relative to the atlas configuration file.
    pub entries: Vec<TextureAtlasEntry>,
}

impl AtlasFile {
    /// Default padding of a texture atlas is 4 pixels.
    #[inline]
    pub const fn default_padding() -> u32 {
        4
    }

    /// Default bleeding of a texture atlas is 4 pixels.
    #[inline]
    pub const fn default_bleeding() -> u32 {
        4
    }

    /// Default usage of texture atlas pages is [RenderAssetUsages::RENDER_WORLD].
    #[inline]
    pub const fn default_usages() -> RenderAssetUsages {
        RenderAssetUsages::RENDER_WORLD
    }

    /// Serializes the usages into `(main: <bool>, render: <bool>)`.
    #[inline]
    pub fn serialize_usages<S: Serializer>(usages: &RenderAssetUsages, ser: S) -> Result<S::Ok, S::Error> {
        let mut u = ser.serialize_struct("RenderAssetUsages", 2)?;
        u.serialize_field("main", &usages.contains(RenderAssetUsages::MAIN_WORLD))?;
        u.serialize_field("render", &usages.contains(RenderAssetUsages::RENDER_WORLD))?;
        u.end()
    }

    /// Deserializes the usages from `(main: <bool>, render: <bool>)`.
    #[inline]
    pub fn deserialize_usages<'de, D: Deserializer<'de>>(de: D) -> Result<RenderAssetUsages, D::Error> {
        const FIELDS: &[&str] = &["main", "render"];

        struct Visit;
        impl<'de> Visitor<'de> for Visit {
            type Value = RenderAssetUsages;

            #[inline]
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(formatter, "struct RenderAssetUsages {{ main: bool, render: bool }}")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where A: MapAccess<'de> {
                let mut main = None::<bool>;
                let mut render = None::<bool>;
                while let Some(key) = map.next_key()? {
                    match key {
                        "main" => match main {
                            None => main = Some(map.next_value()?),
                            Some(..) => return Err(DeError::duplicate_field("main")),
                        },
                        "render" => match render {
                            None => render = Some(map.next_value()?),
                            Some(..) => return Err(DeError::duplicate_field("render")),
                        },
                        e => return Err(DeError::unknown_field(e, FIELDS)),
                    }
                }

                let main = main.ok_or(DeError::missing_field("main"))?;
                let render = render.ok_or(DeError::missing_field("render"))?;
                Ok(match (main, render) {
                    (false, false) => RenderAssetUsages::empty(),
                    (true, false) => RenderAssetUsages::MAIN_WORLD,
                    (false, true) => RenderAssetUsages::RENDER_WORLD,
                    (true, true) => RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
                })
            }
        }

        de.deserialize_struct("RenderAssetUsages", FIELDS, Visit)
    }
}

/// A [Atlas] file entry. May either be a file or a directory containing files.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum TextureAtlasEntry {
    /// Defines a relative path to an [Image] file.
    File(String),
    /// Defines a directory relative to the current one, filled with more entries.
    Directory(String, Vec<TextureAtlasEntry>),
}

impl<T: ToString> From<T> for TextureAtlasEntry {
    #[inline]
    fn from(value: T) -> Self {
        Self::File(value.to_string())
    }
}

/// Additional settings that may be adjusted when loading a [Atlas]. This is typically used
/// to limit texture sizes to what the rendering backend supports.
#[derive(Serialize, Deserialize, Debug, Copy, Clone)]
pub struct TextureAtlasSettings {
    /// The initial width of an atlas page. Gradually grows to [Self::max_width] if insufficient.
    pub init_width: u32,
    /// The initial height of an atlas page. Gradually grows to [Self::max_height] if insufficient.
    pub init_height: u32,
    /// The maximum width of an atlas page. If insufficient, a new page must be allocated.
    pub max_width: u32,
    /// The maximum height of an atlas page. If insufficient, a new page must be allocated.
    pub max_height: u32,
}

impl Default for TextureAtlasSettings {
    #[inline]
    fn default() -> Self {
        Self {
            init_width: 1024,
            init_height: 1024,
            max_width: 4096,
            max_height: 4096,
        }
    }
}

/// Errors that may arise when loading a [`Atlas`].
#[derive(Error, Debug, Display, From)]
pub enum TextureAtlasError {
    /// Error that arises when a texture is larger than the maximum size of the atlas page.
    #[display("Texture '{}' is too large: [{actual_width}, {actual_height}] > [{max_width}, {max_height}]", path.display())]
    TooLarge {
        /// The sprite lookup key.
        path: PathBuf,
        /// The maximum width of the atlas page. See [`TextureAtlasSettings::max_width`].
        max_width: u32,
        /// The maximum width of the atlas page. See [`TextureAtlasSettings::max_height`].
        max_height: u32,
        /// The width of the erroneous texture.
        actual_width: u32,
        /// The height of the erroneous texture.
        actual_height: u32,
    },
    /// Error that arises when the texture couldn't be converted into
    /// [`TextureFormat::Rgba8UnormSrgb`].
    #[display("Texture '{}' has an unsupported format: {format:?}", path.display())]
    UnsupportedFormat {
        /// The sprite lookup key.
        path: PathBuf,
        /// The invalid texture format.
        format: TextureFormat,
    },
    /// Error that arises when the texture couldn't be loaded at all.
    #[display("Texture '{}' failed to load: {error}", path.display())]
    InvalidImage {
        /// The sprite lookup key.
        path: PathBuf,
        /// The error that arises when trying to load the texture.
        error: LoadDirectError,
    },
    /// Error that arises when a texture has an invalid path string.
    #[display("{_0}")]
    InvalidPath(#[from] ParseAssetPathError),
    /// Error that arises when the `.atlas` file has an invalid RON syntax.
    #[display("{_0}")]
    InvalidFile(#[from] SpannedError),
    /// Error that arises when an IO error occurs.
    #[display("{_0}")]
    Io(#[from] io::Error),
}

/// Dedicated [`AssetLoader`] to load [`Atlas`].
///
/// Parses file into [`AtlasFile`]
/// representation, and accepts [`TextureAtlasSettings`] as additional optional configuration. May
/// throw [`TextureAtlasError`] for erroneous assets.
///
/// This asset loader adds each texture atlas entry as a "load dependency." As much, coupled with a
/// file system watcher, mutating these input image files will cause reprocessing of the atlas.
///
/// This asset loader also adds layouts and images as labelled assets with label `"page-{i}"` and
/// `"layout-{i}"` (without the brackets). Therefore, doing (for example)
/// `server.load::<Image>("sprites.atlas.ron#page-0")` is possible and will return the 0th page
/// image of the atlas, provided the atlas actually has a 0th page (which it won't only if there are
/// no sprites at all!).
#[derive(Debug, Copy, Clone, Default)]
pub struct AtlasLoader;
impl AssetLoader for AtlasLoader {
    type Asset = Atlas;
    type Settings = TextureAtlasSettings;
    type Error = TextureAtlasError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        settings: &Self::Settings,
        load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let &Self::Settings {
            init_width,
            init_height,
            max_width,
            max_height,
        } = settings;

        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;

        let AtlasFile {
            padding,
            bleeding,
            usages,
            entries: file_entries,
        } = ron::de::from_bytes(&bytes)?;

        drop(bytes);
        let pad = padding as usize;
        let bleed = (bleeding as usize).min(pad);

        async fn collect(
            prefix: &Path,
            entry: TextureAtlasEntry,
            base: &AssetPath<'_>,
            load_context: &mut LoadContext<'_>,
            accum: &mut Vec<(PathBuf, Image, bool)>,
        ) -> Result<(), TextureAtlasError> {
            match entry {
                TextureAtlasEntry::File(path) => {
                    let path = base.resolve(&path)?;
                    let Some(name) = path.path().file_stem() else { return Ok(()) };

                    let mut name = name.to_string_lossy().into_owned();
                    let has_nine_slice = if let Some((split, "9")) = name.rsplit_once('.') {
                        name = String::from(split);
                        true
                    } else {
                        false
                    };

                    let path_buf = path.resolve_embed(&name)?.path().strip_prefix(prefix).unwrap().to_owned();
                    let src = match load_context.loader().immediate().load::<Image>(&path).await {
                        Err(error) => return Err(TextureAtlasError::InvalidImage { path: path_buf, error }),
                        Ok(src) => src,
                    }
                    .take();

                    accum.push((path_buf, src, has_nine_slice));
                }
                TextureAtlasEntry::Directory(dir, paths) => {
                    let base = base.resolve(&dir)?;
                    for path in paths {
                        Box::pin(collect(prefix, path, &base, load_context, accum)).await?
                    }
                }
            }

            Ok(())
        }

        let prefix = load_context.asset_path().parent().unwrap_or_else(|| AssetPath::from(""));

        let mut entries = Vec::new();
        for file_entry in file_entries {
            collect(prefix.path(), file_entry, &prefix, load_context, &mut entries).await?;
        }

        entries.sort_by_key(|&(.., ref texture, has_nine_slice)| {
            let UVec2 { mut x, mut y } = texture.size();
            if has_nine_slice {
                x = x.saturating_sub(2);
                y = y.saturating_sub(2);
            }

            2 * (x + y)
        });

        let mut output_atlas = Atlas {
            pages: Vec::new(),
            sprites: HashMap::with_hasher(FixedHasher),
        };

        let mut push_page = |ids: HashMap<AllocId, (PathBuf, Image, bool)>, packer: AtlasAllocator| {
            let Size2D {
                width: page_width,
                height: page_height,
                ..
            } = packer.size().to_u32();

            let pixel_size = TextureFormat::Rgba8UnormSrgb.pixel_size();
            let mut sprites = Vec::with_capacity(ids.len());
            let mut data = vec![0; page_width as usize * page_height as usize * pixel_size];

            for (id, (name, texture, has_nine_slice)) in ids {
                let Box2D { min, max } = packer[id].to_usize();
                let nine_offset = usize::from(has_nine_slice);

                let Some(texture) = texture.convert(TextureFormat::Rgba8UnormSrgb) else {
                    return Err(TextureAtlasError::UnsupportedFormat {
                        path: name,
                        format: texture.texture_descriptor.format,
                    });
                };

                let texture = texture.data.as_ref().unwrap();

                let rect_width = max.x - min.x;
                let rect_height = max.y - min.y;

                let src_row = rect_width - 2 * pad;
                let src_pos = |x, y| ((y + nine_offset) * (src_row + nine_offset) + (x + nine_offset)) * pixel_size;

                let dst_row = page_width as usize;
                let dst_pos = |x, y| ((min.y + y) * dst_row + (min.x + x)) * pixel_size;

                // Set topleft-wards bleeding to topleft pixel and topright-wards bleeding to topright pixel. This
                // is so that the subsequent bleeding operation may just use a split-off copy.
                for bleed_x in 0..bleed {
                    data[dst_pos(pad - bleed_x - 1, pad - bleed)..][..pixel_size]
                        .copy_from_slice(&texture[src_pos(0, 0)..][..pixel_size]);

                    data[dst_pos(rect_width - pad + bleed_x, pad - bleed)..][..pixel_size]
                        .copy_from_slice(&texture[src_pos(src_row - 1, 0)..][..pixel_size]);
                }

                // Copy top-most edge to bleed upwards.
                data[dst_pos(pad, pad - bleed)..][..src_row * pixel_size]
                    .copy_from_slice(&texture[src_pos(0, 0)..][..src_row * pixel_size]);
                for bleed_y in 1..bleed {
                    let split = dst_pos(pad - bleed, pad - bleed + bleed_y);
                    let (src, dst) = data.split_at_mut(split);

                    let count = (src_row + 2 * bleed) * pixel_size;
                    dst[..count].copy_from_slice(&src[split - dst_row * pixel_size..][..count]);
                }

                // Copy the actual image, while performing sideways bleeding.
                for y in 0..rect_height - 2 * pad {
                    let count = src_row * pixel_size;
                    data[dst_pos(pad, pad + y)..][..count].copy_from_slice(&texture[src_pos(0, y)..][..count]);

                    for bleed_x in 0..bleed {
                        data[dst_pos(pad - bleed_x - 1, pad + y)..][..pixel_size]
                            .copy_from_slice(&texture[src_pos(0, y)..][..pixel_size]);

                        data[dst_pos(rect_width - pad + bleed_x, pad + y)..][..pixel_size]
                            .copy_from_slice(&texture[src_pos(src_row - 1, y)..][..pixel_size]);
                    }
                }

                // Copy the bottom-most edge to bleed downwards.
                for bleed_y in 0..bleed {
                    let split = dst_pos(pad - bleed, rect_height - pad + bleed_y);
                    let (src, dst) = data.split_at_mut(split);

                    let count = (src_row + 2 * bleed) * pixel_size;
                    dst[..count].copy_from_slice(&src[split - dst_row * pixel_size..][..count]);
                }

                // Finally, insert to the sprite map.
                sprites.push((
                    name,
                    URect {
                        min: uvec2(min.x as u32 + padding, min.y as u32 + padding),
                        max: uvec2(max.x as u32 - padding, max.y as u32 - padding),
                    },
                    // `atlas` and `atlas_page` are to be initialized when all sprites are inserted.
                    AtlasSprite {
                        atlas: TextureAtlas {
                            layout: Handle::Weak(AssetId::invalid()),
                            index: 0,
                        },
                        atlas_page: Handle::Weak(AssetId::invalid()),
                        nine_slices: has_nine_slice.then(|| {
                            let mut cuts = NineSliceCuts {
                                left: 0,
                                right: 0,
                                top: src_row as u32,
                                bottom: rect_height as u32 - 2 * padding,
                            };

                            let mut found_left = false;
                            for x in 1..src_row + 1 {
                                let alpha = texture[x * pixel_size + 3];
                                if !found_left && alpha >= 127 {
                                    found_left = true;
                                    cuts.left = x as u32;
                                } else if found_left && alpha < 127 {
                                    cuts.right = x as u32;
                                    break
                                }
                            }

                            let mut found_top = false;
                            for y in 1..rect_height - 2 * pad + 1 {
                                let alpha = texture[y * (src_row + nine_offset) * pixel_size + 3];
                                if !found_top && alpha >= 127 {
                                    found_top = true;
                                    cuts.top = y as u32;
                                } else if found_top && alpha < 127 {
                                    cuts.bottom = y as u32;
                                    break
                                }
                            }

                            cuts
                        }),
                    },
                ));
            }

            let page_num = output_atlas.pages.len();
            let page = load_context.add_labeled_asset(
                format!("page-{page_num}"),
                Image::new(
                    Extent3d {
                        width: page_width,
                        height: page_height,
                        depth_or_array_layers: 1,
                    },
                    TextureDimension::D2,
                    data,
                    TextureFormat::Rgba8UnormSrgb,
                    usages,
                ),
            );

            // Fill the layout first before turning it into a handle.
            let mut layout = TextureAtlasLayout {
                size: uvec2(page_width, page_height),
                textures: Vec::with_capacity(sprites.len()),
            };

            for (.., rect, entry) in &mut sprites {
                entry.atlas_page = page.clone_weak();
                entry.atlas.index = layout.textures.len();
                layout.textures.push(*rect);
            }

            // Finally turn the layout into a handle, and assign it to sprite entries.
            let layout = load_context.add_labeled_asset(format!("layout-{page_num}"), layout);
            for (name, .., mut entry) in sprites {
                entry.atlas.layout = layout.clone_weak();
                output_atlas.sprites.insert(name, entry);
            }

            output_atlas.pages.push((layout, page));
            Ok(())
        };

        'pages: while !entries.is_empty() {
            let mut packer = AtlasAllocator::new(size2(init_width as i32, init_height as i32));
            let mut ids = HashMap::<AllocId, (PathBuf, Image, bool)>::with_hasher(FixedHasher);

            while let Some((name, texture, has_nine_slice)) = entries.pop() {
                let UVec2 {
                    x: mut base_width,
                    y: mut base_height,
                } = texture.size();

                if has_nine_slice {
                    base_width = base_width.saturating_sub(1);
                    base_height = base_height.saturating_sub(1);
                }

                match packer.allocate(size2(
                    (base_width + 2 * pad as u32) as i32,
                    (base_height + 2 * pad as u32) as i32,
                )) {
                    Some(alloc) => {
                        info!("Packed {}: {:?}", name.display(), alloc.rectangle);
                        ids.insert(alloc.id, (name, texture, has_nine_slice));
                    }
                    None => {
                        let Size2D { width, height, .. } = packer.size();
                        if width == max_width as i32 && height == max_height as i32 {
                            if packer.is_empty() {
                                return Err(TextureAtlasError::TooLarge {
                                    path: name,
                                    max_width,
                                    max_height,
                                    actual_width: width as u32,
                                    actual_height: height as u32,
                                });
                            } else {
                                push_page(ids, packer)?;

                                // Re-insert the entry to the back, since we didn't end up packing that one.
                                entries.push((name, texture, has_nine_slice));
                                continue 'pages;
                            }
                        }

                        let ChangeList { changes, failures } = packer.resize_and_rearrange(size2(
                            (width * 2).min(max_width as i32),
                            (height * 2).min(max_height as i32),
                        ));

                        if !failures.is_empty() {
                            unreachable!("resizing shouldn't cause rectangles to become unfittable")
                        }

                        let mut id_map = HashMap::with_hasher(FixedHasher);
                        for Change { old, new } in changes {
                            let rect = ids.remove(&old.id).unwrap();
                            id_map.insert(new.id, rect);
                        }

                        if !ids.is_empty() {
                            unreachable!("resizing should clear all old rectangles")
                        }

                        ids = id_map;
                    }
                }
            }

            push_page(ids, packer)?;
        }

        Ok(output_atlas)
    }

    #[inline]
    fn extensions(&self) -> &[&str] {
        &["atlas.ron"]
    }
}
