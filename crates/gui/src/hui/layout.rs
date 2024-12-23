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
pub use Val::*;

use crate::gui::{Gui, GuiLayout, PreferredSize};

#[derive(Copy, Clone)]
pub enum Val {
    Px(f32),
    Frac(f32),
    Auto,
}

impl Default for Val {
    #[inline]
    fn default() -> Self {
        Px(0.)
    }
}

impl Val {
    #[inline]
    pub fn get(self) -> f32 {
        match self {
            Px(px) => px,
            Frac(..) | Auto => 0.,
        }
    }

    #[inline]
    pub fn by(self, parent: f32, children: f32) -> f32 {
        match self {
            Px(px) => px,
            Frac(frac) => frac * parent,
            Auto => children,
        }
    }

    #[inline]
    pub fn by_parent(self, parent: f32) -> f32 {
        match self {
            Px(px) => px,
            Frac(frac) => frac * parent,
            Auto => 0.,
        }
    }
}

#[derive(Copy, Clone, Default)]
pub struct ValSize {
    pub x: Val,
    pub y: Val,
}

impl ValSize {
    #[inline]
    pub const fn all(value: Val) -> Self {
        Self { x: value, y: value }
    }

    #[inline]
    pub const fn new(x: Val, y: Val) -> Self {
        Self { x, y }
    }
}

#[derive(Copy, Clone, Default)]
pub struct ValRect {
    pub left: Val,
    pub right: Val,
    pub top: Val,
    pub bottom: Val,
}

impl ValRect {
    #[inline]
    pub const fn all(value: Val) -> Self {
        Self {
            left: value,
            right: value,
            top: value,
            bottom: value,
        }
    }

    #[inline]
    pub const fn xy(x: Val, y: Val) -> Self {
        Self {
            left: x,
            right: x,
            top: y,
            bottom: y,
        }
    }

    #[inline]
    pub const fn new(left: Val, right: Val, top: Val, bottom: Val) -> Self {
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
pub struct Size(pub ValSize);
impl Default for Size {
    #[inline]
    fn default() -> Self {
        Self(ValSize::all(Auto))
    }
}

#[derive(Component, Copy, Clone, Default, Deref, DerefMut)]
#[require(Gui)]
pub struct Margin(pub ValRect);

#[derive(Component, Copy, Clone, Default, Deref, DerefMut)]
#[require(Gui)]
pub struct Padding(pub ValRect);

impl GuiLayout for Cont {
    type Changed = Or<(Changed<Size>, Changed<Margin>, Changed<Padding>)>;

    type InitialParam = SQuery<Read<Margin>>;
    type InitialItem = (Read<Self>, Read<PreferredSize>, Option<Read<Size>>, Option<Read<Padding>>);

    type DistributeParam = ();
    type DistributeItem = ();

    fn initial_layout_size(
        margin_query: &SystemParamItem<Self::InitialParam>,
        (&cont, &preferred_size, size, padding): QueryItem<Self::InitialItem>,
        children: &[Entity],
        children_layout_sizes: &[Vec2],
    ) -> Vec2 {
        let padding = *padding.copied().unwrap_or_default();
        let padding_size = vec2(
            padding.left.get() + padding.right.get(),
            padding.top.get() + padding.bottom.get(),
        );

        let size = size.map(|&size| *size).unwrap_or_default();
        let size = match size {
            ValSize { x: Auto, .. } | ValSize { y: Auto, .. } => {
                let ValSize { x, y } = size;
                let children_size = children
                    .iter()
                    .zip(children_layout_sizes)
                    .fold(Vec2::ZERO, |mut out, (&e, &size)| {
                        let margin = *margin_query.get(e).copied().unwrap_or_default();
                        let margin_x = margin.left.get() + margin.right.get();
                        let margin_y = margin.top.get() + margin.bottom.get();

                        match cont {
                            Self::Horizontal | Self::HorizontalReverse => {
                                out.x += size.x + margin_x;
                                out.y = out.y.max(size.y) + margin_y;
                            }
                            Self::Vertical | Self::VerticalReverse => {
                                out.x = out.x.max(size.x + margin_x);
                                out.y += size.y + margin_y;
                            }
                        }

                        out
                    });

                vec2(x.by(0., children_size.x), y.by(0., children_size.y))
            }
            ValSize { x, y } => vec2(x.get(), y.get()),
        };

        let size = vec2(size.x.max(preferred_size.x), size.y.max(preferred_size.y));
        size + padding_size
    }

    fn distribute_space(
        available_space: Vec2,
        param: &SystemParamItem<Self::DistributeParam>,
        parent: QueryItem<Self::DistributeItem>,
        children: &[Entity],
        output: &mut [(Affine2, Vec2)],
    ) {
        todo!()
    }
}
