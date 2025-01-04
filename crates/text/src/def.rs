use std::io::Error as IoError;

use async_channel::Sender;
use bevy_asset::{io::Reader, prelude::*, AssetLoader, LoadContext};
use bevy_color::prelude::*;
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

#[derive(Component, Reflect, Copy, Clone, Deref, DerefMut)]
#[reflect(Component, Default)]
pub struct TextColor(pub Color);
impl Default for TextColor {
    #[inline]
    fn default() -> Self {
        Self(Color::WHITE)
    }
}

#[derive(Component, Clone, Deref, DerefMut, Default)]
pub struct TextStructure(SmallVec<[Entity; 1]>);

#[derive(Component, Clone)]
pub struct TextGlyphs {
    pub glyphs: Vec<TextGlyph>,
    pub size: Vec2,
    pub(crate) buffer: Buffer,
}

impl Default for TextGlyphs {
    #[inline]
    fn default() -> Self {
        Self {
            glyphs: Vec::new(),
            size: Vec2::ZERO,
            buffer: Buffer::new_empty(Metrics::new(f32::MIN_POSITIVE, f32::MIN_POSITIVE)),
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
        Ref<Parent>,
        Option<Ref<Children>>,
    )>,
    parent_query: Query<&Parent>,
    mut removed_span: RemovedComponents<TextSpan>,
    mut iterated: Local<FixedBitSet>,
    mut removed: Local<FixedBitSet>,
) {
    iterated.clear();
    removed.clear();

    for e in removed_span.read() {
        removed.grow_and_insert((e.index() + 1) as usize);
    }

    'out: for (e, text, span, parent, children) in &changed_query {
        iterated.grow((e.index() + 1) as usize);
        if iterated.put((e.index() + 1) as usize) {
            continue 'out
        }

        let parent_changed = parent.is_changed();
        let children_changed = children.is_some_and(|children| children.is_changed());

        if match (text.as_ref(), span) {
            (Some(text), ..) => text.is_added() || children_changed,
            (None, Some(span)) => span.is_added() || parent_changed || children_changed,
            (None, None) => {
                if removed.contains((e.index() + 1) as usize) {
                    true
                } else {
                    continue 'out
                }
            }
        } {
            let (root, structure, children) = if text.is_some() {
                match text_query.get_mut(e) {
                    Ok(structure) => structure,
                    Err(..) => continue 'out,
                }
            } else {
                let mut e = parent.get();
                loop {
                    iterated.grow((e.index() + 1) as usize);
                    if iterated.put((e.index() + 1) as usize) {
                        continue 'out
                    }

                    match text_query.get_mut(e) {
                        Ok(structure) => break structure,
                        Err(..) => match parent_query.get(e) {
                            Ok(parent) => {
                                e = parent.get();
                                continue
                            }
                            Err(..) => continue 'out,
                        },
                    }
                }
            };

            fn recurse(
                structure: &mut SmallVec<[Entity; 1]>,
                parent: Entity,
                children: &[Entity],
                recurse_query: &Query<(Entity, Option<&Children>, &Parent), (With<TextSpan>, Without<Text>)>,
            ) {
                for (e, children, actual_parent) in recurse_query.iter_many(children) {
                    assert_eq!(
                        actual_parent.get(), parent,
                        "Malformed hierarchy. This probably means that your hierarchy has been improperly maintained, or contains a cycle"
                    );

                    structure.push(e);
                    if let Some(children) = children {
                        recurse(structure, e, children, recurse_query);
                    }
                }
            }

            let structure = structure.into_inner();
            structure.clear();
            structure.push(root);
            if let Some(children) = children {
                recurse(structure, root, children, &recurse_query);
            }
        }
    }
}
