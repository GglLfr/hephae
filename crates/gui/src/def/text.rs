use bevy_asset::prelude::*;
use bevy_ecs::{
    prelude::*,
    query::QueryItem,
    system::{
        SystemParamItem,
        lifetimeless::{Read, SQuery, SRes},
    },
};
use bevy_image::prelude::*;
use bevy_math::{Affine2, prelude::*, vec2};
use hephae_text::{atlas::FontAtlas, def::TextQuery, prelude::*};

use crate::{
    def::{Margin, UiSize, UiVal::*, UiVal2},
    gui::{GuiLayout, GuiSize},
};

/// UI layout that can be used for texts.
///
/// Additional components may be added to this node:
/// - [`UiSize`] for specifying the size.
/// - [`Margin`] for offsetting.
///
/// Note that adding [`Shrink`](crate::def::Shrink) may be unfavorable, as the shrinking mechanism
/// isn't recognized by the text layout to increase its height in case of text wrapping. In general,
/// if a node's width is [`Auto`], never expect text wrapping to work.
#[derive(Component, Copy, Clone, Default)]
#[require(Text)]
pub struct UiText;

impl GuiLayout for UiText {
    type Changed = Changed<TextStructure>;

    type PrimaryParam = (
        SRes<FontLayout>,
        SRes<Assets<Font>>,
        SQuery<(
            Option<Read<Text>>,
            Option<Read<TextSpan>>,
            Option<Read<TextFont>>,
        )>,
    );
    type PrimaryItem = (
        Read<Text>,
        Read<TextGlyphs>,
        Read<TextStructure>,
        Option<Read<UiSize>>,
        Option<Read<Margin>>,
    );

    type SecondaryParam = (
        SRes<FontLayout>,
        SRes<Assets<Font>>,
        SQuery<(
            Option<Read<Text>>,
            Option<Read<TextSpan>>,
            Option<Read<TextFont>>,
        )>,
    );
    type SecondaryItem = (
        Read<Text>,
        Read<TextGlyphs>,
        Read<TextStructure>,
        Option<Read<UiSize>>,
        Option<Read<Margin>>,
    );

    type DistributeParam = ();
    type DistributeItem = Option<Read<Margin>>;

    fn primary_layout_size(
        (layout, fonts, query): &SystemParamItem<Self::PrimaryParam>,
        (text, glyphs, structure, size, margin): QueryItem<Self::PrimaryItem>,
        _: &[Entity],
        _: &[Vec2],
    ) -> Vec2 {
        let margin = *margin.copied().unwrap_or_default();

        let size = size.map(|&size| *size).unwrap_or_default();
        let size = match size {
            UiVal2 { x: Auto, .. } | UiVal2 { y: Auto, .. } => 'size: {
                let UiVal2 { x, y } = size;
                let (width, height) = match (x, y) {
                    (Abs(val), Auto) => (Some(val), None),
                    (Auto, Abs(val)) => (None, Some(val)),
                    (Auto, Auto) => (None, None),
                    (Rel(..), Abs(val)) => break 'size vec2(0., val),
                    (Rel(..), Auto) => break 'size Vec2::ZERO,
                    (Abs(val), Rel(..)) => break 'size vec2(val, 0.),
                    (Auto, Rel(..)) => break 'size Vec2::ZERO,
                    _ => unreachable!("either x or y is Auto"),
                };

                let mut size = layout
                    .get()
                    .measure_glyphs(
                        glyphs,
                        (width, height),
                        text.wrap,
                        text.align,
                        1.,
                        fonts,
                        structure.iter(query),
                    )
                    .unwrap_or(Vec2::ZERO);

                if let Some(width) = width {
                    size.x = width;
                }

                if let Some(height) = height {
                    size.y = height;
                }

                size
            }
            UiVal2 { x, y } => vec2(x.refer_rel(0.), y.refer_rel(0.)),
        };

        size + margin.size()
    }

    fn secondary_layout_size(
        (layout, fonts, query): &SystemParamItem<Self::SecondaryParam>,
        (mut this, (text, glyphs, structure, size, margin)): (Vec2, QueryItem<Self::SecondaryItem>),
        parent: Vec2,
    ) -> (Vec2, Vec2) {
        let size = size.map(|&size| *size).unwrap_or_default();
        let margin = *margin.copied().unwrap_or_default();

        if let Some((width, height)) = match (size.x, size.y) {
            (Rel(..), Abs(val)) => Some((
                Some(size.x.refer_rel(parent.x)),
                Some(val + (margin.top + margin.bottom)),
            )),
            (Rel(..), Auto) => Some((Some(size.x.refer_rel(parent.x)), None)),
            (Abs(val), Rel(..)) => Some((
                Some(val + (margin.left + margin.right)),
                Some(size.y.refer_rel(parent.y)),
            )),
            (Auto, Rel(..)) => Some((None, Some(size.y.refer_rel(parent.y)))),
            _ => None,
        } {
            let mut size = layout
                .get()
                .measure_glyphs(
                    glyphs,
                    (
                        width.map(|w| w - (margin.left + margin.right)),
                        height.map(|h| h - (margin.top + margin.bottom)),
                    ),
                    text.wrap,
                    text.align,
                    1.,
                    fonts,
                    structure.iter(query),
                )
                .unwrap_or(Vec2::ZERO);

            if let Some(width) = width {
                size.x = width;
            }

            if let Some(height) = height {
                size.y = height;
            }

            this = size;
        }

        (this, this - margin.size())
    }

    fn distribute_space(
        (this_transform, this_size): (&mut Affine2, &mut Vec2),
        _: &SystemParamItem<Self::DistributeParam>,
        margin: QueryItem<Self::DistributeItem>,
        _: &[Entity],
        _: &mut [(Affine2, Vec2)],
    ) {
        let margin = *margin.copied().unwrap_or_default();
        *this_transform *= Affine2::from_translation(vec2(margin.left, margin.bottom));
        *this_size = (*this_size - margin.size()).max(Vec2::ZERO);
    }
}

/// Computes GUI text glyphs after a UI recalculation.
pub fn update_text_widget(
    mut layout: ResMut<FontLayout>,
    fonts: Res<Assets<Font>>,
    mut images: ResMut<Assets<Image>>,
    mut atlases: ResMut<Assets<FontAtlas>>,
    mut glyphs_query: Query<
        (&mut TextGlyphs, &Text, &mut TextStructure, &GuiSize),
        Or<(Changed<TextStructure>, Changed<GuiSize>)>,
    >,
    query: TextQuery,
) {
    for (mut glyphs, text, mut structure, &size) in &mut glyphs_query {
        if layout
            .get_mut()
            .compute_glyphs(
                &mut glyphs,
                (Some(size.x), Some(size.y)),
                text.wrap,
                text.align,
                1.,
                &fonts,
                &mut images,
                &mut atlases,
                structure.iter(&query),
            )
            .is_err()
        {
            structure.set_changed();
        }
    }
}
