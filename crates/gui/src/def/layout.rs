use bevy_derive::{Deref, DerefMut};
use bevy_ecs::{
    prelude::*,
    query::QueryItem,
    system::{
        lifetimeless::{Read, SQuery},
        SystemParamItem,
    },
};
use bevy_math::{prelude::*, vec2, Affine2};
pub use UiVal::*;

use crate::gui::{Gui, GuiLayout, PreferredSize};

#[derive(Copy, Clone)]
pub enum UiVal {
    Abs(f32),
    Rel(f32),
    Auto,
}

impl Default for UiVal {
    #[inline]
    fn default() -> Self {
        Abs(0.)
    }
}

impl UiVal {
    #[inline]
    pub fn get(self) -> f32 {
        match self {
            Abs(px) => px,
            Rel(..) | Auto => 0.,
        }
    }

    #[inline]
    pub fn refer(self, ref_frac: f32, ref_auto: f32) -> f32 {
        match self {
            Abs(px) => px,
            Rel(frac) => frac * ref_frac,
            Auto => ref_auto,
        }
    }

    #[inline]
    pub fn refer_frac(self, ref_frac: f32) -> f32 {
        match self {
            Abs(px) => px,
            Rel(frac) => frac * ref_frac,
            Auto => 0.,
        }
    }
}

#[derive(Copy, Clone, Default)]
pub struct UiVal2 {
    pub x: UiVal,
    pub y: UiVal,
}

impl UiVal2 {
    #[inline]
    pub const fn all(value: UiVal) -> Self {
        Self { x: value, y: value }
    }

    #[inline]
    pub const fn new(x: UiVal, y: UiVal) -> Self {
        Self { x, y }
    }
}

#[derive(Copy, Clone, Default)]
pub struct HuiRect {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

impl HuiRect {
    #[inline]
    pub const fn all(value: f32) -> Self {
        Self {
            left: value,
            right: value,
            top: value,
            bottom: value,
        }
    }

    #[inline]
    pub const fn xy(x: f32, y: f32) -> Self {
        Self {
            left: x,
            right: x,
            top: y,
            bottom: y,
        }
    }

    #[inline]
    pub const fn new(left: f32, right: f32, top: f32, bottom: f32) -> Self {
        Self {
            left,
            right,
            top,
            bottom,
        }
    }
}

#[derive(Component, Copy, Clone, Default)]
pub enum Cont {
    #[default]
    Horizontal,
    HorizontalReverse,
    Vertical,
    VerticalReverse,
}

#[derive(Component, Copy, Clone, Deref, DerefMut)]
#[require(Gui)]
pub struct UiSize(pub UiVal2);
impl Default for UiSize {
    #[inline]
    fn default() -> Self {
        Self(UiVal2::all(Auto))
    }
}

impl UiSize {
    #[inline]
    pub const fn all(value: UiVal) -> Self {
        Self(UiVal2 { x: value, y: value })
    }

    #[inline]
    pub const fn new(x: UiVal, y: UiVal) -> Self {
        Self(UiVal2 { x, y })
    }
}

#[derive(Component, Copy, Clone, Default, Deref, DerefMut)]
#[require(Gui)]
pub struct Margin(pub HuiRect);
impl Margin {
    #[inline]
    pub const fn all(value: f32) -> Self {
        Self(HuiRect {
            left: value,
            right: value,
            top: value,
            bottom: value,
        })
    }

    #[inline]
    pub const fn xy(x: f32, y: f32) -> Self {
        Self(HuiRect {
            left: x,
            right: x,
            top: y,
            bottom: y,
        })
    }

    #[inline]
    pub const fn new(left: f32, right: f32, top: f32, bottom: f32) -> Self {
        Self(HuiRect {
            left,
            right,
            top,
            bottom,
        })
    }
}

#[derive(Component, Copy, Clone, Default, Deref, DerefMut)]
#[require(Gui)]
pub struct Padding(pub HuiRect);
impl Padding {
    #[inline]
    pub const fn all(value: f32) -> Self {
        Self(HuiRect {
            left: value,
            right: value,
            top: value,
            bottom: value,
        })
    }

    #[inline]
    pub const fn xy(x: f32, y: f32) -> Self {
        Self(HuiRect {
            left: x,
            right: x,
            top: y,
            bottom: y,
        })
    }

    #[inline]
    pub const fn new(left: f32, right: f32, top: f32, bottom: f32) -> Self {
        Self(HuiRect {
            left,
            right,
            top,
            bottom,
        })
    }
}

#[derive(Component, Copy, Clone, Default, Deref, DerefMut)]
#[require(Gui)]
pub struct Expand(pub Vec2);

#[derive(Component, Copy, Clone, Default, Deref, DerefMut)]
#[require(Gui)]
pub struct Shrink(pub Vec2);

impl GuiLayout for Cont {
    type Changed = Or<(
        Changed<UiSize>,
        Changed<Margin>,
        Changed<Padding>,
        Changed<Expand>,
        Changed<Shrink>,
    )>;

    type InitialParam = ();
    type InitialItem = (
        Read<Self>,
        Read<PreferredSize>,
        Option<Read<UiSize>>,
        Option<Read<Padding>>,
        Option<Read<Margin>>,
    );

    type DistributeParam = (
        SQuery<Option<Read<UiSize>>>,
        SQuery<(Option<Read<Expand>>, Option<Read<Shrink>>)>,
        SQuery<Option<Read<Margin>>>,
    );
    type DistributeItem = (Read<Self>, Option<Read<Padding>>);

    fn initial_layout_size(
        _: &SystemParamItem<Self::InitialParam>,
        (&cont, &preferred_size, size, padding, margin): QueryItem<Self::InitialItem>,
        _: &[Entity],
        children_layout_sizes: &[Vec2],
    ) -> Vec2 {
        let size = size.map(|&size| *size).unwrap_or_default();
        let size = match size {
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

                vec2(
                    x.refer(0., children_size.x.max(preferred_size.x)),
                    y.refer(0., children_size.y.max(preferred_size.y)),
                )
            }
            UiVal2 { x, y } => vec2(x.get(), y.get()),
        };

        let padding = *padding.copied().unwrap_or_default();
        let margin = *margin.copied().unwrap_or_default();

        size + vec2(padding.left + padding.right, padding.top + padding.bottom) +
            vec2(margin.left + margin.right, margin.top + margin.bottom)
    }

    fn distribute_space(
        available_space: Vec2,
        (size_query, flex_query, margin_query): &SystemParamItem<Self::DistributeParam>,
        (&cont, padding): QueryItem<Self::DistributeItem>,
        children: &[Entity],
        output: &mut [(Affine2, Vec2)],
    ) {
        let padding = *padding.copied().unwrap_or_default();
        let available_space = available_space - vec2(padding.left + padding.right, padding.top + padding.bottom);

        let (taken, mut total_expand, mut total_shrink) = children.iter().zip(output.iter_mut()).fold(
            (Vec2::ZERO, Vec2::ZERO, Vec2::ZERO),
            |(mut taken, mut total_expand, mut total_shrink), (&child, (.., initial_size))| {
                let size = size_query.get(child).unwrap();
                let (expand, shrink) = flex_query.get(child).unwrap();

                total_expand += *expand.copied().unwrap_or_default();
                total_shrink += *shrink.copied().unwrap_or_default();

                let size = size.map(|&size| *size).unwrap_or_default();
                let size = vec2(
                    match size.x {
                        Rel(frac) => frac * available_space.x,
                        _ => initial_size.x,
                    },
                    match size.y {
                        Rel(frac) => frac * available_space.y,
                        _ => initial_size.y,
                    },
                );

                *initial_size = size;
                match cont {
                    Self::Horizontal | Self::HorizontalReverse => {
                        taken.x += size.x;
                        taken.y = taken.y.max(size.y);
                    }
                    Self::Vertical | Self::VerticalReverse => {
                        taken.x = taken.x.max(size.x);
                        taken.y += size.y;
                    }
                }

                (taken, total_expand, total_shrink)
            },
        );

        total_expand = total_expand.max(Vec2::ONE);
        total_shrink = total_shrink.max(Vec2::ONE);

        let delta = available_space - taken;
        let delta_expand = delta.max(Vec2::ZERO);
        let delta_shrink = delta.min(Vec2::ZERO);

        let mut offset = match cont {
            Self::Horizontal | Self::Vertical => vec2(0., available_space.y),
            Self::HorizontalReverse => vec2(available_space.x, available_space.y),
            Self::VerticalReverse => Vec2::ZERO,
        } + vec2(padding.left, padding.bottom);

        for (&child, (trns, output)) in children.iter().zip(output.iter_mut()) {
            let (expand, shrink) = flex_query.get(child).unwrap();
            let margin = *margin_query.get(child).unwrap().copied().unwrap_or_default();

            let size = *output +
                delta_expand * (*expand.copied().unwrap_or_default() / total_expand) +
                delta_shrink * (*shrink.copied().unwrap_or_default() / total_shrink);

            let pos = offset +
                match cont {
                    Self::Horizontal | Self::Vertical => Vec2::ZERO,
                    Self::HorizontalReverse => vec2(-size.x, 0.),
                    Self::VerticalReverse => vec2(0., size.y),
                };

            *trns = Affine2::from_translation(pos + vec2(margin.left, margin.bottom - size.y));
            *output = size - vec2(margin.left + margin.right, margin.top + margin.bottom);

            offset += match cont {
                Self::Horizontal => vec2(size.x, 0.),
                Self::HorizontalReverse => vec2(-size.x, 0.),
                Self::Vertical => vec2(0., -size.y),
                Self::VerticalReverse => vec2(0., size.y),
            };
        }
    }
}
