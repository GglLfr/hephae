//! Defines font layout computation systems.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use async_channel::{Receiver, Sender};
use bevy::{
    platform::{collections::HashMap, hash::FixedHasher},
    prelude::*,
    tasks::IoTaskPool,
};
use cosmic_text::{
    Attrs, Buffer, CacheKey, Family, FontSystem, Metrics, Shaping, SwashCache,
    fontdb::{Database, Source},
    ttf_parser::{Face, FaceParsingError},
};
use derive_more::{Display, Error};
use scopeguard::{Always, ScopeGuard};

use crate::{
    atlas::{FontAtlas, FontAtlasKey, FontAtlases},
    def::{Font, TextAlign, TextFont, TextGlyph, TextGlyphs, TextWrap},
};

/// Global handle to the font layout, wrapped in a mutex.
#[derive(Resource)]
pub struct FontLayout(pub(crate) Mutex<FontLayoutInner>);
impl FontLayout {
    /// Gets a reference to the inner resource. When possible, always prefer [`Self::get_mut`].
    #[inline]
    pub fn get(&self) -> MutexGuard<FontLayoutInner> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Gets a reference to the inner resource.
    #[inline]
    pub fn get_mut(&mut self) -> &mut FontLayoutInner {
        self.0.get_mut().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Handles computations for font glyphs.
pub struct FontLayoutInner {
    sys: FontSystem,
    cache: SwashCache,
    pending_fonts: Receiver<(Vec<u8>, Sender<Result<Font, FaceParsingError>>)>,
    font_atlases: HashMap<AssetId<Font>, FontAtlases>,
    spans: Vec<(&'static str, &'static TextFont)>,
    glyph_spans: Vec<(AssetId<Font>, FontAtlasKey)>,
}

impl FontLayoutInner {
    /// Creates a new font layout pipeline.
    #[inline]
    pub(crate) fn new(pending_fonts: Receiver<(Vec<u8>, Sender<Result<Font, FaceParsingError>>)>) -> Self {
        let locale = sys_locale::get_locale().unwrap_or("en-US".into());
        Self {
            sys: FontSystem::new_with_locale_and_db(locale, Database::new()),
            cache: SwashCache::new(),
            pending_fonts,
            font_atlases: HashMap::with_hasher(FixedHasher),
            spans: Vec::new(),
            glyph_spans: Vec::new(),
        }
    }
}

/// loads bytes sent from [`FontLoader`](crate::def::FontLoader) into a [`Font`] and adds them to
/// the database.
pub fn load_fonts_to_database(mut fonts: ResMut<FontLayout>) {
    let fonts = fonts.get_mut();
    while let Ok((bytes, sender)) = fonts.pending_fonts.try_recv() {
        if let Err(e) = Face::parse(&bytes, 0) {
            IoTaskPool::get().spawn(async move { _ = sender.send(Err(e)).await }).detach();
            continue
        }

        // None of these unwraps should fail, as `Face::parse` has already ensured a valid 0th face.
        let src = Arc::new(bytes.into_boxed_slice());
        let id = fonts.sys.db_mut().load_font_source(Source::Binary(src))[0];

        let info = fonts.sys.db().face(id).unwrap();
        let name = info
            .families
            .first()
            .map(|(name, _)| name)
            .cloned()
            .unwrap_or("Times New Roman".into());
        let style = info.style;
        let weight = info.weight;
        let stretch = info.stretch;

        IoTaskPool::get()
            .spawn(async move {
                _ = sender
                    .send(Ok(Font {
                        id,
                        name,
                        style,
                        weight,
                        stretch,
                    }))
                    .await
            })
            .detach()
    }
}

/// Errors that may arise from font computations.
#[derive(Error, Debug, Display)]
pub enum FontLayoutError {
    /// A font has not been loaded yet.
    #[display("required font hasn't been loaded yet or has failed loading")]
    FontNotLoaded(#[error(not(source))] AssetId<Font>),
    /// Couldn't get an image for a glyph.
    #[display("couldn't render an image for a glyph")]
    NoGlyphImage(#[error(not(source))] CacheKey),
}

impl FontLayoutInner {
    /// Computes [`TextGlyphs`].
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
        spans: impl Iterator<Item = (&'a str, &'a TextFont)>,
    ) -> Result<(), FontLayoutError> {
        glyphs.size = Vec2::ZERO;
        glyphs.glyphs.clear();

        let mut glyph_spans = std::mem::take(&mut self.glyph_spans);
        let spans = spans.inspect(|&(.., font)| {
            glyph_spans.push((font.font.id(), FontAtlasKey {
                font_size: font.font_size.to_bits(),
                antialias: font.antialias,
            }))
        });

        let buffer = glyphs.buffer.get_mut().unwrap_or_else(PoisonError::into_inner);
        if let Err(e) = self.update_buffer(buffer, (width, height), wrap, align, scale_factor, fonts, spans) {
            glyph_spans.clear();
            self.glyph_spans = glyph_spans;

            return Err(e);
        }

        let buffer_size = buffer_size(buffer);
        if let Err::<(), FontLayoutError>(e) = buffer
            .layout_runs()
            .flat_map(|run| run.glyphs.iter().map(move |glyph| (glyph, run.line_y)))
            .try_for_each(|(glyph, line)| {
                let (id, key) = glyph_spans[glyph.metadata];

                let mut tmp;
                let glyph = if !key.antialias {
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

            return Err(e);
        }

        glyph_spans.clear();
        self.glyph_spans = glyph_spans;

        glyphs.size = buffer_size;
        Ok(())
    }

    /// Gets the box size of a text.
    #[inline]
    pub fn measure_glyphs<'a>(
        &mut self,
        glyphs: &TextGlyphs,
        (width, height): (Option<f32>, Option<f32>),
        wrap: TextWrap,
        align: TextAlign,
        scale_factor: f32,
        fonts: &Assets<Font>,
        spans: impl Iterator<Item = (&'a str, &'a TextFont)>,
    ) -> Result<Vec2, FontLayoutError> {
        let mut buffer = glyphs.buffer.lock().unwrap_or_else(PoisonError::into_inner);
        self.update_buffer(&mut buffer, (width, height), wrap, align, scale_factor, fonts, spans)?;

        Ok(buffer_size(&buffer))
    }

    /// Gets the box size of a precomputed text.
    #[inline]
    pub fn size(&self, glyphs: &TextGlyphs) -> Vec2 {
        buffer_size(&glyphs.buffer.lock().unwrap_or_else(PoisonError::into_inner))
    }

    fn update_buffer<'a>(
        &mut self,
        buffer: &mut Buffer,
        (width, height): (Option<f32>, Option<f32>),
        wrap: TextWrap,
        align: TextAlign,
        scale_factor: f32,
        fonts: &Assets<Font>,
        spans: impl Iterator<Item = (&'a str, &'a TextFont)>,
    ) -> Result<(), FontLayoutError> {
        /// Delegates [`std::mem::transmute`] to shrink the vector element's lifetime, but with
        /// invariant mutable reference lifetime to the vector so it may not be accessed while the
        /// guard is active.
        ///
        /// # Safety:
        /// - The guard must **not** be passed anywhere else. Ideally, you'd want to immediately
        ///   dereference it just to make sure.
        /// - The drop glue of the guard must be called, i.e., [`std::mem::forget`] may not be
        ///   called. This is to ensure the `'a` lifetime objects are cleared out.
        #[inline]
        #[allow(unsafe_op_in_unsafe_fn)]
        unsafe fn guard<'a, 'this: 'a>(
            spans: &'this mut Vec<(&'static str, &'static TextFont)>,
        ) -> ScopeGuard<&'this mut Vec<(&'a str, &'a TextFont)>, fn(&mut Vec<(&'a str, &'a TextFont)>), Always> {
            // Safety: We only change the lifetime, so the value is valid for both types.
            ScopeGuard::with_strategy(
                std::mem::transmute::<
                    &'this mut Vec<(&'static str, &'static TextFont)>,
                    &'this mut Vec<(&'a str, &'a TextFont)>,
                >(spans),
                Vec::clear,
            )
        }

        // Safety: The guard is guaranteed not to be dropped early since it's immediately dereferenced.
        let spans_vec = &mut **unsafe { guard(&mut self.spans) };
        let sys = &mut self.sys;

        let mut font_size = f32::MIN_POSITIVE;
        for (span, font) in spans {
            if span.is_empty() || font.font_size <= 0. || font.line_height <= 0. {
                continue;
            }

            if !fonts.contains(&font.font) {
                return Err(FontLayoutError::FontNotLoaded(font.font.id()));
            }

            font_size = font_size.max(font.font_size);
            spans_vec.push((span, font));
        }

        let mut buffer = buffer.borrow_with(sys);
        buffer.lines.clear();
        buffer.set_metrics_and_size(Metrics::relative(font_size, 1.2).scale(scale_factor), width, height);
        buffer.set_wrap(wrap.into());
        buffer.set_rich_text(
            spans_vec.iter().enumerate().map(|(span_index, &(span, font))| {
                // The unwrap won't fail because the existence of the fonts have been checked.
                let info = fonts.get(&font.font).unwrap();
                (
                    span,
                    Attrs::new()
                        .family(Family::Name(&info.name))
                        .stretch(info.stretch)
                        .style(info.style)
                        .weight(info.weight)
                        .metadata(span_index)
                        .metrics(Metrics::relative(font.font_size, font.line_height)),
                )
            }),
            &Attrs::new(),
            Shaping::Advanced,
            Some(align.into()),
        );

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
