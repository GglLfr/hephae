use bevy_ecs::{
    prelude::*,
    query::QueryItem,
    system::{
        lifetimeless::{Read, SQuery, SRes},
        SystemParamItem,
    },
};
use bevy_math::{vec2, Affine2, Vec2};
use hephae_text::prelude::*;

use crate::{
    def::{UiSize, UiVal::*, UiVal2},
    gui::GuiLayout,
};

#[derive(Component, Copy, Clone, Default)]
#[require(Text)]
pub struct UiText;

impl GuiLayout for UiText {
    type Changed = Changed<TextStructure>;

    type InitialParam = (
        SRes<FontLayout>,
        SQuery<(Option<Read<Text>>, Option<Read<TextSpan>>, Option<Read<TextFont>>)>,
    );
    type InitialItem = (Read<TextStructure>, Read<TextGlyphs>, Option<Read<UiSize>>);

    type DistributeParam = ();
    type DistributeItem = ();

    fn initial_layout_size(
        (layout, query): &SystemParamItem<Self::InitialParam>,
        (structure, glyphs, size): QueryItem<Self::InitialItem>,
        children: &[Entity],
        children_layout_sizes: &[Vec2],
    ) -> Vec2 {
        /*let size = size.map(|&size| *size).unwrap_or_default();
        let size = match size {
            UiVal2 { x: Auto, .. } | UiVal2 { y: Auto, .. } => {
                let UiVal2 { x, y } = size;
                layout.get().measure_glyphs(
                    glyphs,
                    (match x {
                        Abs(val) => Some(val),
                        Auto => None,
                    }),
                )
            }
            UiVal2 { x, y } => vec2(x.refer_rel(0.), y.refer_rel(0.)),
        };*/
        /*
        UiVal2 { x: Auto, .. } | UiVal2 { y: Auto, .. } => {
            let UiVal2 { x, y } = size;
            let children_size = children_layout_sizes.iter().fold(Vec2::ZERO, |mut out, &size| {
                match cont {
                    Self::Horizontal | Self::HorizontalReverse => {
                        out.x += size.x;
                        out.y = out.y.max(size.y);
                    }
                    Self::Vertical | Self::VerticalReverse => {
                        out.x = out.x.max(size.x);
                        out.y += size.y;
                    }
                }

                out
            });

            vec2(x.refer(0., children_size.x), y.refer(0., children_size.y))
        }
        UiVal2 { x, y } => vec2(x.refer_rel(0.), y.refer_rel(0.)),
         */

        todo!()
    }

    fn distribute_space(
        this: (&mut Affine2, &mut Vec2),
        param: &SystemParamItem<Self::DistributeParam>,
        parent: QueryItem<Self::DistributeItem>,
        children: &[Entity],
        output: &mut [(Affine2, Vec2)],
    ) {
        todo!()
    }
}
