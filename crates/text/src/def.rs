//! Defines common types of Hephae Text.

use std::{io, slice::Iter, sync::Mutex};

use async_channel::Sender;
use bevy::{
    asset::{AssetLoader, LoadContext, io::Reader},
    ecs::system::{
        SystemParamItem,
        lifetimeless::{Read, SQuery},
    },
    prelude::*,
};
use cosmic_text::{
    Align, Buffer, Metrics, Stretch, Style, Weight, Wrap, fontdb::ID as FontId, ttf_parser::FaceParsingError,
};
use derive_more::{Display, Error, From};
use fixedbitset::FixedBitSet;
#[cfg(feature = "locale")]
use hephae_locale::prelude::*;
use smallvec::SmallVec;

use crate::atlas::FontAtlas;

/// A TTF font asset.
#[derive(Asset, TypePath, Clone)]
pub struct Font {
    /// The font ID in the database.
    pub id: FontId,
    /// The family name of the font.
    pub name: String,
    /// The font style.
    pub style: Style,
    /// The font weight.
    pub weight: Weight,
    /// The font stretch.
    pub stretch: Stretch,
}

/// Errors that may arise when loading a [`Font`].
#[derive(Error, Debug, Display, From)]
pub enum FontError {
    /// The async channel between the asset thread and main world thread has been closed.
    #[display("the async channel to add fonts to the database was closed")]
    ChannelClosed,
    /// An IO error occurred.
    #[display("{_0}")]
    Io(#[from] io::Error),
    /// Invalid asset bytes.
    #[display("{_0}")]
    Face(#[from] FaceParsingError),
}

/// Asset loader for [`Font`]s.
pub struct FontLoader {
    pub(crate) add_to_database: Sender<(Vec<u8>, Sender<Result<Font, FaceParsingError>>)>,
}

impl AssetLoader for FontLoader {
    type Asset = Font;
    type Settings = ();
    type Error = FontError;

    #[inline]
    async fn load(
        &self,
        reader: &mut dyn Reader,
        _: &Self::Settings,
        _: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;

        let (sender, receiver) = async_channel::bounded(1);
        if self.add_to_database.send((bytes, sender)).await.is_err() {
            return Err(FontError::ChannelClosed);
        }

        let font = receiver.recv().await.map_err(|_| FontError::ChannelClosed)??;
        Ok(font)
    }

    #[inline]
    fn extensions(&self) -> &[&str] {
        &["ttf"]
    }
}

/// Main text component. May have children entities that have [`TextSpan`] and [`TextFont`]
/// component.
#[derive(Component, Reflect, Clone, Default, Deref, DerefMut)]
#[reflect(Component, Default)]
#[require(TextStructure, TextGlyphs)]
pub struct Text {
    /// The text span.
    #[deref]
    pub text: String,
    /// Defines how the text should wrap in case of insufficient space.
    pub wrap: TextWrap,
    /// Defines how the text should align over extra horizontal space.
    pub align: TextAlign,
}

impl Text {
    /// Convenience method to create a new text without wrapping and with left-align.
    #[inline]
    pub fn new(text: impl ToString) -> Self {
        Self {
            text: text.to_string(),
            ..Self::default()
        }
    }
}

#[cfg(feature = "locale")]
impl LocaleTarget for Text {
    #[inline]
    fn update(&mut self, src: &str) {
        src.clone_into(self);
    }
}

/// Defines how the text should wrap in case of insufficient space.
#[derive(Reflect, Eq, PartialEq, Copy, Clone, Default)]
#[reflect(Default)]
pub enum TextWrap {
    /// Don't wrap.
    #[default]
    None,
    /// Individual letters may wrap, similar to the behavior seen in command-lines.
    Glyph,
    /// Individual words may wrap, similar to the behavior seen in text documents.
    Word,
    /// Words or letters may wrap, preferring the former.
    WordOrGlyph,
}

impl From<TextWrap> for Wrap {
    #[inline]
    fn from(value: TextWrap) -> Self {
        match value {
            TextWrap::None => Self::None,
            TextWrap::Glyph => Self::Glyph,
            TextWrap::Word => Self::Word,
            TextWrap::WordOrGlyph => Self::WordOrGlyph,
        }
    }
}

/// Defines how the text should align over extra horizontal space.
#[derive(Reflect, Default, Eq, PartialEq, Clone, Copy)]
#[reflect(Default)]
pub enum TextAlign {
    /// Aligns the text left.
    #[default]
    Left,
    /// Aligns the text right.
    Right,
    /// Aligns the text at the center.
    Center,
    /// Similar to left, but makes the left and right border of the text parallel.
    Justified,
    /// Aligns right for left-to-right fonts, left otherwise.
    End,
}

impl From<TextAlign> for Align {
    #[inline]
    fn from(value: TextAlign) -> Self {
        match value {
            TextAlign::Left => Self::Left,
            TextAlign::Right => Self::Right,
            TextAlign::Center => Self::Center,
            TextAlign::Justified => Self::Justified,
            TextAlign::End => Self::End,
        }
    }
}

/// Defines the font of a [`Text`] or a [`TextSpan`].
#[derive(Component, Reflect, Clone)]
#[reflect(Component, Default)]
pub struct TextFont {
    /// The font handle.
    pub font: Handle<Font>,
    /// The font size. Note that frequently changing this will result in high memory usage.
    pub font_size: f32,
    /// The relative line height of the font.
    pub line_height: f32,
    /// Whether to antialias the font glyphs.
    pub antialias: bool,
}

impl Default for TextFont {
    #[inline]
    fn default() -> Self {
        Self {
            font: Handle::default(),
            font_size: 16.,
            line_height: 1.2,
            antialias: true,
        }
    }
}

/// Type of [`Query`] that may be passed to [`TextStructure::iter`], with a `'static` lifetime.
pub type STextQuery = SQuery<(Option<Read<Text>>, Option<Read<TextSpan>>, Option<Read<TextFont>>)>;
/// Type of [`Query`] that may be passed to [`TextStructure::iter`].
pub type TextQuery<'w, 's> = SystemParamItem<'w, 's, STextQuery>;

/// Contains entities that make up a full text buffer. Listen to [`Changed<TextStructure>`] if you
/// want to recompute [`TextGlyphs`].
#[derive(Component, Clone, Deref, DerefMut, Default)]
pub struct TextStructure(SmallVec<[(Entity, usize); 1]>);
impl TextStructure {
    /// Iterates textual entities for use in [`FontLayout`](crate::layout::FontLayout).
    #[inline]
    pub fn iter<'w, 's>(&'w self, query: &'w TextQuery<'w, 's>) -> TextStructureIter<'w, 's> {
        TextStructureIter {
            inner: self.0.iter(),
            fonts: SmallVec::new_const(),
            query,
        }
    }
}

/// Iterates textual entities for use in [`FontLayout`](crate::layout::FontLayout).
pub struct TextStructureIter<'w, 's: 'w> {
    inner: Iter<'w, (Entity, usize)>,
    fonts: SmallVec<[(&'w TextFont, usize); 4]>,
    query: &'w TextQuery<'w, 's>,
}

impl<'w> Iterator for TextStructureIter<'w, '_> {
    type Item = (&'w str, &'w TextFont);

    fn next(&mut self) -> Option<Self::Item> {
        const DEFAULT_FONT: TextFont = TextFont {
            font: Handle::Weak(AssetId::Uuid {
                uuid: AssetId::<Font>::DEFAULT_UUID,
            }),
            font_size: 24.,
            line_height: 1.2,
            antialias: true,
        };

        let &(e, depth) = self.inner.next()?;
        let (text, span, font) = self.query.get(e).ok()?;
        let str = match (text, span) {
            (Some(text), ..) => text.as_str(),
            (None, Some(span)) => span.as_str(),
            (None, None) => return None,
        };

        let font = font.unwrap_or_else(|| {
            loop {
                let &(last_font, last_depth) = self.fonts.last().unwrap_or(&(&DEFAULT_FONT, 0));
                if depth > 0 && last_depth >= depth {
                    self.fonts.pop();
                } else {
                    self.fonts.push((last_font, depth));
                    break last_font;
                }
            }
        });

        Some((str, font))
    }
}

/// Contains the computed glyphs of a text entity.
#[derive(Component)]
pub struct TextGlyphs {
    /// The glyphs, ready to be rendered.
    pub glyphs: Vec<TextGlyph>,
    /// The size of the text.
    pub size: Vec2,
    pub(crate) buffer: Mutex<Buffer>,
}

impl Default for TextGlyphs {
    #[inline]
    fn default() -> Self {
        Self {
            glyphs: Vec::new(),
            size: Vec2::ZERO,
            buffer: Mutex::new(Buffer::new_empty(Metrics::new(f32::MIN_POSITIVE, f32::MIN_POSITIVE))),
        }
    }
}

/// A single information about how a glyph can be rendered.
#[derive(Clone)]
pub struct TextGlyph {
    /// Positional offset of the glyph relative to the text box's bottom-left corner.
    pub origin: Vec2,
    /// The size of this glyph.
    pub size: Vec2,
    /// The atlas that this glyph uses.
    pub atlas: AssetId<FontAtlas>,
    /// The index of this glyph information in its [`FontAtlas`].
    pub index: usize,
}

/// May be added to child entities of [`Text`].
#[derive(Component, Reflect, Default, Deref, DerefMut)]
#[reflect(Component, Default)]
pub struct TextSpan(pub String);

#[cfg(feature = "locale")]
impl LocaleTarget for TextSpan {
    #[inline]
    fn update(&mut self, src: &str) {
        src.clone_into(self);
    }
}

/// Computes and marks [`TextStructure`] as changed as necessary, for convenience of systems
/// wishing to listen for change-detection.
pub fn compute_structure(
    mut text_query: Query<(Entity, &mut TextStructure, Option<&Children>)>,
    recurse_query: Query<(Entity, Option<&Children>, &ChildOf), (With<TextSpan>, Without<Text>)>,
    changed_query: Query<(
        Entity,
        Option<Ref<Text>>,
        Option<Ref<TextSpan>>,
        Option<Ref<ChildOf>>,
        Option<Ref<Children>>,
    )>,
    parent_query: Query<&ChildOf>,
    mut removed_span: RemovedComponents<TextSpan>,
    mut iterated: Local<FixedBitSet>,
    mut removed: Local<FixedBitSet>,
    mut old: Local<SmallVec<[(Entity, usize); 1]>>,
) {
    iterated.clear();
    removed.clear();

    for e in removed_span.read() {
        removed.grow_and_insert(e.index() as usize);
    }

    'out: for (e, text, span, parent, children) in &changed_query {
        iterated.grow((e.index() + 1) as usize);
        if iterated.put(e.index() as usize) {
            continue 'out;
        }

        let parent_changed = parent.as_ref().is_some_and(Ref::is_changed);
        let children_changed = children.is_some_and(|children| children.is_changed());

        if match (text.as_ref(), span) {
            (Some(text), ..) => text.is_added() || children_changed,
            (None, Some(span)) => span.is_added() || parent_changed || children_changed,
            (None, None) => {
                if removed.contains(e.index() as usize) {
                    true
                } else {
                    continue 'out;
                }
            }
        } {
            let Ok((root, mut structure, children)) = (if text.is_some() {
                text_query.get_mut(e)
            } else {
                let Some(mut e) = parent.map(|p| p.parent) else {
                    continue 'out;
                };
                loop {
                    iterated.grow((e.index() + 1) as usize);
                    if iterated.put(e.index() as usize) {
                        continue 'out;
                    }

                    match text_query.get_mut(e) {
                        Ok(structure) => break Ok(structure),
                        Err(..) => match parent_query.get(e) {
                            Ok(parent) => {
                                e = parent.parent;
                                continue;
                            }
                            Err(..) => continue 'out,
                        },
                    }
                }
            }) else {
                continue 'out;
            };

            let inner = &mut structure.bypass_change_detection().0;
            old.append(inner);

            fn recurse(
                structure: &mut SmallVec<[(Entity, usize); 1]>,
                depth: usize,
                parent: Entity,
                children: &[Entity],
                recurse_query: &Query<(Entity, Option<&Children>, &ChildOf), (With<TextSpan>, Without<Text>)>,
            ) {
                for (e, children, actual_parent) in recurse_query.iter_many(children) {
                    assert_eq!(
                        actual_parent.parent, parent,
                        "Malformed hierarchy. This probably means that your hierarchy has been improperly maintained, or contains a cycle"
                    );

                    structure.push((e, depth));
                    if let Some(children) = children {
                        recurse(structure, depth + 1, e, children, recurse_query);
                    }
                }
            }

            inner.clear();
            inner.push((root, 0));
            if let Some(children) = children {
                recurse(inner, 1, root, children, &recurse_query);
            }

            if &*old != inner {
                structure.set_changed();
            }

            old.clear();
        }
    }
}

/// Computes and marks [`TextStructure`] as changed as necessary, for convenience of systems
/// wishing to listen for change-detection.
pub fn notify_structure(
    mut root_query: Query<&mut TextStructure>,
    changed_query: Query<
        (Option<Ref<Text>>, Option<Ref<TextSpan>>, Option<Ref<TextFont>>),
        Or<(With<Text>, With<TextSpan>)>,
    >,
) {
    'out: for mut structure in &mut root_query {
        let inner = &structure.bypass_change_detection().0;
        for (text, span, font) in changed_query.iter_many(inner.iter().map(|&(e, ..)| e)) {
            if text.is_some_and(|text| text.is_changed()) ||
                span.is_some_and(|span| span.is_changed()) ||
                font.is_some_and(|font| font.is_changed())
            {
                structure.set_changed();
                continue 'out;
            }
        }
    }
}
