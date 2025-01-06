use std::{io::Error as IoError, slice::Iter, sync::Mutex};

use async_channel::Sender;
use bevy_asset::{io::Reader, prelude::*, AssetLoader, LoadContext};
use bevy_derive::{Deref, DerefMut};
use bevy_ecs::prelude::*;
use bevy_hierarchy::prelude::*;
use bevy_math::prelude::*;
use bevy_reflect::prelude::*;
use cosmic_text::{
    fontdb::ID as FontId, ttf_parser::FaceParsingError, Align, Buffer, Metrics, Stretch, Style, Weight, Wrap,
};
use fixedbitset::FixedBitSet;
use smallvec::SmallVec;
use thiserror::Error;

use crate::atlas::FontAtlas;

#[derive(Asset, TypePath, Clone)]
pub struct Font {
    pub id: FontId,
    pub name: String,
    pub style: Style,
    pub weight: Weight,
    pub stretch: Stretch,
}

#[derive(Error, Debug)]
pub enum FontError {
    #[error("the async channel to add fonts to the database was closed")]
    ChannelClosed,
    #[error(transparent)]
    Io(#[from] IoError),
    #[error(transparent)]
    Face(#[from] FaceParsingError),
}

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
            return Err(FontError::ChannelClosed)
        }

        let font = receiver.recv().await.map_err(|_| FontError::ChannelClosed)??;
        Ok(font)
    }

    #[inline]
    fn extensions(&self) -> &[&str] {
        &["ttf"]
    }
}

#[derive(Component, Reflect, Clone, Default)]
#[reflect(Component, Default)]
#[require(TextStructure, TextGlyphs)]
pub struct Text {
    pub text: String,
    pub wrap: TextWrap,
    pub align: TextAlign,
}

impl Text {
    #[inline]
    pub fn new(text: impl ToString) -> Self {
        Self {
            text: text.to_string(),
            ..Self::default()
        }
    }
}

#[derive(Reflect, Eq, PartialEq, Copy, Clone, Default)]
#[reflect(Default)]
pub enum TextWrap {
    #[default]
    None,
    Glyph,
    Word,
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

#[derive(Reflect, Default, Eq, PartialEq, Clone, Copy)]
#[reflect(Default)]
pub enum TextAlign {
    #[default]
    Left,
    Right,
    Center,
    Justified,
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

#[derive(Component, Reflect, Clone)]
#[reflect(Component, Default)]
pub struct TextFont {
    pub font: Handle<Font>,
    pub font_size: f32,
    pub line_height: f32,
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

#[derive(Component, Clone, Deref, DerefMut, Default)]
pub struct TextStructure(SmallVec<[(Entity, usize); 1]>);
impl TextStructure {
    #[inline]
    pub fn iter<'w, 's, 'a, 'b, 'c>(
        &'w self,
        query: &'w Query<'w, 's, (Option<&'a Text>, Option<&'b TextSpan>, Option<&'c TextFont>)>,
    ) -> TextStructureIter<'w, 's, 'a, 'b, 'c> {
        TextStructureIter {
            inner: self.0.iter(),
            fonts: SmallVec::new_const(),
            query,
        }
    }
}

pub struct TextStructureIter<'w, 's, 'a, 'b, 'c> {
    inner: Iter<'w, (Entity, usize)>,
    fonts: SmallVec<[(&'w TextFont, usize); 1]>,
    query: &'w Query<'w, 's, (Option<&'a Text>, Option<&'b TextSpan>, Option<&'c TextFont>)>,
}

impl<'w, 's, 'a, 'b, 'c> Iterator for TextStructureIter<'w, 's, 'a, 'b, 'c> {
    type Item = (&'w str, &'w TextFont);

    fn next(&mut self) -> Option<Self::Item> {
        static DEFAULT_FONT: TextFont = TextFont {
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
            (Some(text), ..) => text.text.as_str(),
            (None, Some(span)) => span.0.as_str(),
            (None, None) => return None,
        };

        let font = font.unwrap_or_else(|| loop {
            let &(last_font, last_depth) = self.fonts.last().unwrap_or(&(&DEFAULT_FONT, 0));
            if depth > 0 && last_depth >= depth {
                self.fonts.pop();
            } else {
                self.fonts.push((last_font, depth));
                break last_font
            }
        });

        Some((str, font))
    }
}

#[derive(Component)]
pub struct TextGlyphs {
    pub glyphs: Vec<TextGlyph>,
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

#[derive(Copy, Clone)]
pub struct TextGlyph {
    pub origin: Vec2,
    pub size: Vec2,
    pub atlas: AssetId<FontAtlas>,
    pub index: usize,
}

#[derive(Component, Reflect, Default)]
#[reflect(Component, Default)]
pub struct TextSpan(pub String);

pub fn compute_structure(
    mut text_query: Query<(Entity, &mut TextStructure, Option<&Children>)>,
    recurse_query: Query<(Entity, Option<&Children>, &Parent), (With<TextSpan>, Without<Text>)>,
    changed_query: Query<(
        Entity,
        Option<Ref<Text>>,
        Option<Ref<TextSpan>>,
        Option<Ref<Parent>>,
        Option<Ref<Children>>,
    )>,
    parent_query: Query<&Parent>,
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
            continue 'out
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
                    continue 'out
                }
            }
        } {
            let Ok((root, mut structure, children)) = (if text.is_some() {
                text_query.get_mut(e)
            } else {
                let Some(mut e) = parent.map(|p| p.get()) else { continue 'out };
                loop {
                    iterated.grow((e.index() + 1) as usize);
                    if iterated.put(e.index() as usize) {
                        continue 'out
                    }

                    match text_query.get_mut(e) {
                        Ok(structure) => break Ok(structure),
                        Err(..) => match parent_query.get(e) {
                            Ok(parent) => {
                                e = parent.get();
                                continue
                            }
                            Err(..) => continue 'out,
                        },
                    }
                }
            }) else {
                continue 'out
            };

            let inner = &mut structure.bypass_change_detection().0;
            old.append(inner);

            fn recurse(
                structure: &mut SmallVec<[(Entity, usize); 1]>,
                depth: usize,
                parent: Entity,
                children: &[Entity],
                recurse_query: &Query<(Entity, Option<&Children>, &Parent), (With<TextSpan>, Without<Text>)>,
            ) {
                for (e, children, actual_parent) in recurse_query.iter_many(children) {
                    assert_eq!(
                        actual_parent.get(), parent,
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
                continue 'out
            }
        }
    }
}
