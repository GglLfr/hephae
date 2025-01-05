use std::sync::Arc;

use async_channel::{Receiver, Sender};
use bevy_asset::prelude::*;
use bevy_color::prelude::*;
use bevy_ecs::prelude::*;
use bevy_image::prelude::*;
use bevy_math::prelude::*;
use bevy_utils::HashMap;
use cosmic_text::{
    fontdb::{Database, Source},
    ttf_parser::{Face, FaceParsingError},
    Attrs, Buffer, CacheKey, Color, Family, FontSystem, Metrics, Shaping, SwashCache,
};
use thiserror::Error;

use crate::{
    atlas::{FontAtlas, FontAtlasKey, FontAtlases},
    def::{Font, TextAlign, TextFont, TextGlyph, TextGlyphs, TextWrap},
};

#[derive(Resource)]
pub struct FontLayout {
    sys: FontSystem,
    cache: SwashCache,
    pending_fonts: Receiver<(Vec<u8>, Sender<Result<Font, FaceParsingError>>)>,
    font_atlases: HashMap<AssetId<Font>, FontAtlases>,
    spans: Vec<(&'static str, &'static TextFont, LinearRgba)>,
    glyph_spans: Vec<(AssetId<Font>, FontAtlasKey)>,
}

impl FontLayout {
    #[inline]
    pub fn new(pending_fonts: Receiver<(Vec<u8>, Sender<Result<Font, FaceParsingError>>)>) -> Self {
        let locale = sys_locale::get_locale().unwrap_or("en-US".into());
        Self {
            sys: FontSystem::new_with_locale_and_db(locale, Database::new()),
            cache: SwashCache::new(),
            pending_fonts,
            font_atlases: HashMap::new(),
            spans: Vec::new(),
            glyph_spans: Vec::new(),
        }
    }
}

pub fn load_fonts_to_database(fonts: ResMut<FontLayout>) {
    let fonts = fonts.into_inner();
    while let Ok((bytes, sender)) = fonts.pending_fonts.try_recv() {
        if let Err(e) = Face::parse(&bytes, 0) {
            _ = sender.send_blocking(Err(e));
            continue
        }

        // None of these unwraps should fail, as `Face::parse` has already ensured a valid 0th face.
        let src = Arc::new(bytes.into_boxed_slice());
        let id = fonts.sys.db_mut().load_font_source(Source::Binary(src))[0];
        let info = fonts.sys.db().face(id).unwrap();

        _ = sender.send_blocking(Ok(Font {
            id,
            name: info
                .families
                .get(0)
                .map(|(name, _)| name)
                .cloned()
                .unwrap_or("Times New Roman".into()),
            style: info.style,
            weight: info.weight,
            stretch: info.stretch,
        }));
    }
}

#[derive(Error, Debug)]
pub enum FontLayoutError {
    #[error("required font hasn't been loaded yet or has failed loading")]
    FontNotLoaded(AssetId<Font>),
    #[error("couldn't render an image for a glyph")]
    NoGlyphImage(CacheKey),
}

impl FontLayout {
    pub fn compute_glyphs<'a>(
        &mut self,
        glyphs: &mut TextGlyphs,
        (width, height): (Option<f32>, Option<f32>),
        wrap: TextWrap,
        align: TextAlign,
        scale_factor: f32,
        fonts: &Assets<Font>,
        images: &mut Assets<Image>,
        atlases: &mut Assets<FontAtlas>,
        spans: impl Iterator<Item = (&'a str, &'a TextFont, LinearRgba)>,
    ) -> Result<(), FontLayoutError> {
        glyphs.size = Vec2::ZERO;
        glyphs.glyphs.clear();

        let mut glyph_spans = std::mem::take(&mut self.glyph_spans);
        let spans = spans.inspect(|&(_, font, _)| {
            glyph_spans.push((font.font.id(), FontAtlasKey {
                font_size: font.font_size.to_bits(),
                antialias: font.antialias,
            }))
        });

        if let Err(e) = self.update_buffer(glyphs, (width, height), wrap, align, scale_factor, fonts, spans) {
            glyph_spans.clear();
            self.glyph_spans = glyph_spans;

            return Err(e)
        }

        let buffer_size = buffer_size(&mut glyphs.buffer);
        if let Err::<(), FontLayoutError>(e) = glyphs
            .buffer
            .layout_runs()
            .flat_map(|run| run.glyphs.iter().map(move |glyph| (glyph, run.line_y)))
            .try_for_each(|(glyph, line)| {
                let (id, key) = glyph_spans[glyph.metadata];

                let mut tmp;
                let glyph = if key.antialias {
                    tmp = glyph.clone();
                    tmp.x = tmp.x.round();
                    tmp.y = tmp.y.round();
                    tmp.w = tmp.w.round();
                    tmp.x_offset = tmp.x_offset.round();
                    tmp.y_offset = tmp.y_offset.round();
                    tmp.line_height_opt = tmp.line_height_opt.map(f32::round);
                    &tmp
                } else {
                    glyph
                };

                let atlas_set = self.font_atlases.entry(id).or_default();
                let phys = glyph.physical((0., 0.), 1.);
                let (atlas_id, atlas) = atlas_set.atlas_mut(key, atlases);

                let (offset, rect, index) = atlas.get_or_create_info(&mut self.sys, &mut self.cache, glyph, images)?;
                let size = (rect.max - rect.min).as_vec2();
                let (top, left) = (offset.y as f32, offset.x as f32);

                let x = left + phys.x as f32;
                let y = buffer_size.y - (line.round() + phys.y as f32 - (top - size.y));

                glyphs.glyphs.push(TextGlyph {
                    color: glyph
                        .color_opt
                        .map(|color| color.0.to_le_bytes())
                        .unwrap_or([255, 255, 255, 255]),
                    origin: Vec2::new(x, y),
                    size,
                    atlas: atlas_id,
                    index,
                });

                Ok(())
            })
        {
            glyphs.glyphs.clear();
            glyph_spans.clear();
            self.glyph_spans = glyph_spans;

            return Err(e)
        }

        glyph_spans.clear();
        self.glyph_spans = glyph_spans;

        glyphs.size = buffer_size;
        Ok(())
    }

    fn update_buffer<'a>(
        &mut self,
        glyphs: &mut TextGlyphs,
        (width, height): (Option<f32>, Option<f32>),
        wrap: TextWrap,
        align: TextAlign,
        scale_factor: f32,
        fonts: &Assets<Font>,
        spans: impl Iterator<Item = (&'a str, &'a TextFont, LinearRgba)>,
    ) -> Result<(), FontLayoutError> {
        // Safety:
        // - We only change the lifetime, so the value is valid for both types.
        // - `scopeguard` guarantees that any element in this vector is dropped when this function
        // finishes, so the 'a references aren't leaked out.
        // - The scope guard is guaranteed not to be dropped early since it's immediately dereferenced.
        let spans_vec = &mut **scopeguard::guard(
            unsafe {
                std::mem::transmute::<
                    // Write out the input type here so that if the field type is changed and this one isn't, it errors.
                    &mut Vec<(&'static str, &'static TextFont, LinearRgba)>,
                    &mut Vec<(&'a str, &'a TextFont, LinearRgba)>,
                >(&mut self.spans)
            },
            Vec::clear,
        );

        let sys = &mut self.sys;

        let mut font_size = f32::MIN_POSITIVE;
        for (span, font, color) in spans {
            if span.is_empty() || font.font_size <= 0. || font.line_height <= 0. {
                continue
            }

            if !fonts.contains(&font.font) {
                return Err(FontLayoutError::FontNotLoaded(font.font.id()))
            }

            font_size = font_size.max(font.font_size);
            spans_vec.push((span, font, color));
        }

        let mut buffer = glyphs.buffer.borrow_with(sys);
        buffer.lines.clear();
        buffer.set_metrics_and_size(Metrics::relative(font_size, 1.2).scale(scale_factor), width, height);
        buffer.set_wrap(wrap.into());
        buffer.set_rich_text(
            spans_vec.iter().enumerate().map(|(span_index, &(span, font, color))| {
                // The unwrap won't fail because the existence of the fonts have been checked.
                let info = fonts.get(&font.font).unwrap();
                (
                    span,
                    Attrs::new()
                        .color(Color(color.as_u32())) // Technically the format differs here, but `cosmic-text` doesn't really care.
                        .family(Family::Name(&info.name))
                        .stretch(info.stretch)
                        .style(info.style)
                        .weight(info.weight)
                        .metadata(span_index)
                        .metrics(Metrics::relative(font.font_size, font.line_height)),
                )
            }),
            Attrs::new(),
            Shaping::Advanced,
        );

        for line in &mut buffer.lines {
            line.set_align(Some(align.into()));
        }

        buffer.shape_until_scroll(false);
        Ok(())
    }
}

fn buffer_size(buffer: &Buffer) -> Vec2 {
    let (width, height) = buffer
        .layout_runs()
        .map(|run| (run.line_w, run.line_height))
        .reduce(|(w1, h1), (w2, h2)| (w1.max(w2), h1 + h2))
        .unwrap_or((0., 0.));

    Vec2::new(width, height).ceil()
}
