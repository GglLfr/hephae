pub use UiVal::*;
use bevy_derive::{Deref, DerefMut};
use bevy_ecs::{
    prelude::*,
    query::QueryItem,
    system::{
        SystemParamItem,
        lifetimeless::{Read, SQuery},
    },
};
use bevy_math::{Affine2, prelude::*, vec2};

use crate::gui::{Gui, GuiLayout};

/// GUI value measurement use to calculate widget size.
#[derive(Copy, Clone)]
pub enum UiVal {
    /// Absolute units; pixels in 2D, meters in 3D.
    Abs(f32),
    /// Fraction of the parent's size. Note that it's invalid to have [`Rel`] on parents that have
    /// [`Auto`], and will produce garbage values.
    Rel(f32),
    /// Follows the children's size. Note that it's invalid to have [`Auto`] on children that have
    /// [`Rel`], and will produce garbage values.
    Auto,
}

impl Default for UiVal {
    #[inline]
    fn default() -> Self {
        Abs(0.)
    }
}

impl UiVal {
    /// Refers to both the parent and children size accordingly.
    #[inline]
    pub fn refer(self, ref_frac: f32, ref_auto: f32) -> f32 {
        match self {
            Abs(px) => px,
            Rel(frac) => frac * ref_frac,
            Auto => ref_auto,
        }
    }

    /// Refers to the parent size, and returns `0.0` for [`Auto`].
    #[inline]
    pub fn refer_rel(self, ref_frac: f32) -> f32 {
        match self {
            Abs(px) => px,
            Rel(frac) => frac * ref_frac,
            Auto => 0.,
        }
    }
}

/// 2-Dimensional [`UiVal`].
#[derive(Copy, Clone, Default)]
pub struct UiVal2 {
    /// The value in the X axis.
    pub x: UiVal,
    /// The value in the Y axis.
    pub y: UiVal,
}

impl UiVal2 {
    /// Sets both X and Y axes to `value`.
    #[inline]
    pub const fn all(value: UiVal) -> Self {
        Self { x: value, y: value }
    }

    /// Creates a new [`UiVal2`].
    #[inline]
    pub const fn new(x: UiVal, y: UiVal) -> Self {
        Self { x, y }
    }
}

/// Absolute units spanning left, right, top, and bottom sides.
#[derive(Copy, Clone, Default)]
pub struct AbsRect {
    /// The length for the left side.
    pub left: f32,
    /// The length for the right side.
    pub right: f32,
    /// The length for the top side.
    pub top: f32,
    /// The length for the bottom side.
    pub bottom: f32,
}

impl AbsRect {
    /// Sets all sides to `value`.
    #[inline]
    pub const fn all(value: f32) -> Self {
        Self {
            left: value,
            right: value,
            top: value,
            bottom: value,
        }
    }

    /// Sets left and right to `x`, top and bottom to `y`.
    #[inline]
    pub const fn xy(x: f32, y: f32) -> Self {
        Self {
            left: x,
            right: x,
            top: y,
            bottom: y,
        }
    }

    /// Creates a new [`AbsRect`].
    #[inline]
    pub const fn new(left: f32, right: f32, top: f32, bottom: f32) -> Self {
        Self {
            left,
            right,
            top,
            bottom,
        }
    }

    /// Returns a vector where X is `left + right` and Y is `top + bottom`.
    #[inline]
    pub const fn size(self) -> Vec2 {
        vec2(self.left + self.right, self.top + self.bottom)
    }
}

/// A built-in [`GuiLayout`] implementation. Arranges children either horizontally or vertically
/// without wrapping.
///
/// Additional components may be added either to this node or its direct children:
/// - [`UiSize`] for specifying the size.
/// - [`Margin`] and [`Padding`] for offsetting.
/// - [`Expand`] and [`Shrink`] for specifying behavior on either extra or exhausted space.
#[derive(Component, Copy, Clone, Default)]
pub enum UiCont {
    /// Arranges the children left-to-right.
    #[default]
    Horizontal,
    /// Arranges the children right-to-left.
    HorizontalReverse,
    /// Arranges the children top-to-bottom.
    Vertical,
    /// Arranges the children bottom-to-top.
    VerticalReverse,
}

/// Defines the size of this GUI node.
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
    /// Sets both X and Y axes to `value`.
    #[inline]
    pub const fn all(value: UiVal) -> Self {
        Self(UiVal2 { x: value, y: value })
    }

    /// Creates a new [`UiSize`].
    #[inline]
    pub const fn new(x: UiVal, y: UiVal) -> Self {
        Self(UiVal2 { x, y })
    }
}

/// Defines the empty space around this GUI node outside its borders.
#[derive(Component, Copy, Clone, Default, Deref, DerefMut)]
#[require(Gui)]
pub struct Margin(pub AbsRect);
impl Margin {
    /// Sets all sides to `value`.
    #[inline]
    pub const fn all(value: f32) -> Self {
        Self(AbsRect {
            left: value,
            right: value,
            top: value,
            bottom: value,
        })
    }

    /// Sets left and right to `x`, top and bottom to `y`.
    #[inline]
    pub const fn xy(x: f32, y: f32) -> Self {
        Self(AbsRect {
            left: x,
            right: x,
            top: y,
            bottom: y,
        })
    }

    /// Creates a new [`Margin`].
    #[inline]
    pub const fn new(left: f32, right: f32, top: f32, bottom: f32) -> Self {
        Self(AbsRect {
            left,
            right,
            top,
            bottom,
        })
    }
}

/// Defines the empty space around this GUI node inside its borders, i.e., offsetting its children.
#[derive(Component, Copy, Clone, Default, Deref, DerefMut)]
#[require(Gui)]
pub struct Padding(pub AbsRect);
impl Padding {
    /// Sets all sides to `value`.
    #[inline]
    pub const fn all(value: f32) -> Self {
        Self(AbsRect {
            left: value,
            right: value,
            top: value,
            bottom: value,
        })
    }

    /// Sets left and right to `x`, top and bottom to `y`.
    #[inline]
    pub const fn xy(x: f32, y: f32) -> Self {
        Self(AbsRect {
            left: x,
            right: x,
            top: y,
            bottom: y,
        })
    }

    /// Creates a new [`Padding`].
    #[inline]
    pub const fn new(left: f32, right: f32, top: f32, bottom: f32) -> Self {
        Self(AbsRect {
            left,
            right,
            top,
            bottom,
        })
    }
}

/// Defines how much space in fraction should this GUI node take in case of extra space.
#[derive(Component, Copy, Clone, Default, Deref, DerefMut)]
#[require(Gui)]
pub struct Expand(pub Vec2);

/// Defines how much space in fraction should this GUI node give up in case of exhausted space.
#[derive(Component, Copy, Clone, Default, Deref, DerefMut)]
#[require(Gui)]
pub struct Shrink(pub Vec2);

impl GuiLayout for UiCont {
    type Changed = Or<(
        Changed<UiSize>,
        Changed<Margin>,
        Changed<Padding>,
        Changed<Expand>,
        Changed<Shrink>,
    )>;

    type PrimaryParam = ();
    type PrimaryItem = (
        Read<Self>,
        Option<Read<UiSize>>,
        Option<Read<Padding>>,
        Option<Read<Margin>>,
    );

    type SecondaryParam = ();
    type SecondaryItem = (
        Option<Read<UiSize>>,
        Option<Read<Padding>>,
        Option<Read<Margin>>,
    );

    type DistributeParam = SQuery<(Option<Read<Expand>>, Option<Read<Shrink>>)>;
    type DistributeItem = (Read<Self>, Option<Read<Padding>>, Option<Read<Margin>>);

    fn primary_layout_size(
        _: &SystemParamItem<Self::PrimaryParam>,
        (&cont, size, padding, margin): QueryItem<Self::PrimaryItem>,
        _: &[Entity],
        children_layout_sizes: &[Vec2],
    ) -> Vec2 {
        let padding = *padding.copied().unwrap_or_default();
        let margin = *margin.copied().unwrap_or_default();

        let size = size.map(|&size| *size).unwrap_or_default();
        let size = match size {
            UiVal2 { x: Auto, .. } | UiVal2 { y: Auto, .. } => {
                let UiVal2 { x, y } = size;
                let children_size =
                    children_layout_sizes
                        .iter()
                        .fold(Vec2::ZERO, |mut out, &size| {
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
        };

        size + padding.size() + margin.size()
    }

    fn secondary_layout_size(
        _: &SystemParamItem<Self::SecondaryParam>,
        (mut this, (size, padding, margin)): (Vec2, QueryItem<Self::SecondaryItem>),
        parent: Vec2,
    ) -> (Vec2, Vec2) {
        let size = size.map(|&size| *size).unwrap_or_default();
        let padding = *padding.copied().unwrap_or_default();
        let margin = *margin.copied().unwrap_or_default();

        if let Rel(..) = size.x {
            this.x = size.x.refer_rel(parent.x);
        }

        if let Rel(..) = size.y {
            this.y = size.y.refer_rel(parent.y);
        }

        (this, this - padding.size() - margin.size())
    }

    fn distribute_space(
        (this_transform, this_size): (&mut Affine2, &mut Vec2),
        flex_query: &SystemParamItem<Self::DistributeParam>,
        (&cont, padding, margin): QueryItem<Self::DistributeItem>,
        children: &[Entity],
        output: &mut [(Affine2, Vec2)],
    ) {
        let margin = *margin.copied().unwrap_or_default();
        *this_transform *= Affine2::from_translation(vec2(margin.left, margin.bottom));
        *this_size = (*this_size - margin.size()).max(Vec2::ZERO);

        let padding = *padding.copied().unwrap_or_default();
        let available_space = (*this_size - padding.size()).max(Vec2::ZERO);

        let (taken, mut total_expand, mut total_shrink) =
            children.iter().zip(output.iter_mut()).fold(
                (0., 0., 0.),
                |(mut taken, mut total_expand, mut total_shrink), (&child, &mut (.., size))| {
                    let (expand, shrink) = flex_query.get(child).unwrap();
                    let expand = *expand.copied().unwrap_or_default();
                    let shrink = *shrink.copied().unwrap_or_default();

                    match cont {
                        Self::Horizontal | Self::HorizontalReverse => {
                            taken += size.x;
                            total_expand += expand.x;
                            total_shrink += shrink.x;
                        }
                        Self::Vertical | Self::VerticalReverse => {
                            taken += size.y;
                            total_expand += expand.y;
                            total_shrink += shrink.y;
                        }
                    }

                    (taken, total_expand, total_shrink)
                },
            );

        total_expand = total_expand.max(1.);
        total_shrink = total_shrink.max(1.);

        let delta = match cont {
            Self::Horizontal | Self::HorizontalReverse => available_space.x,
            Self::Vertical | Self::VerticalReverse => available_space.y,
        } - taken;
        let delta_expand = delta.max(0.);
        let delta_shrink = delta.min(0.);

        let mut offset = match cont {
            Self::Horizontal | Self::Vertical => vec2(0., available_space.y),
            Self::HorizontalReverse => vec2(available_space.x, available_space.y),
            Self::VerticalReverse => Vec2::ZERO,
        } + vec2(padding.left, padding.bottom);

        for (&child, (trns, output)) in children.iter().zip(output.iter_mut()) {
            let (expand, shrink) = flex_query.get(child).unwrap();
            let expand = *expand.copied().unwrap_or_default();
            let shrink = *shrink.copied().unwrap_or_default();

            let cross = match cont {
                Self::Horizontal | Self::HorizontalReverse => available_space.y - output.y,
                Self::Vertical | Self::VerticalReverse => available_space.x - output.x,
            };
            let cross_expand = cross.max(0.);
            let cross_shrink = cross.min(0.);

            let size = (*output
                + match cont {
                    Self::Horizontal | Self::HorizontalReverse => vec2(
                        delta_expand * (expand.x / total_expand)
                            + delta_shrink * (shrink.x / total_shrink),
                        cross_expand * expand.y + cross_shrink * shrink.y,
                    ),
                    Self::Vertical | Self::VerticalReverse => vec2(
                        cross_expand * expand.x + cross_shrink * shrink.x,
                        delta_expand * (expand.y / total_expand)
                            + delta_shrink * (shrink.y / total_shrink),
                    ),
                })
            .max(Vec2::ZERO);

            let pos = offset
                + match cont {
                    Self::Horizontal | Self::Vertical => Vec2::ZERO,
                    Self::HorizontalReverse => vec2(-size.x, 0.),
                    Self::VerticalReverse => vec2(0., size.y),
                };

            *trns = Affine2::from_translation(pos - vec2(0., size.y));
            *output = size;

            offset += match cont {
                Self::Horizontal => vec2(size.x, 0.),
                Self::HorizontalReverse => vec2(-size.x, 0.),
                Self::Vertical => vec2(0., -size.y),
                Self::VerticalReverse => vec2(0., size.y),
            };
        }
    }
}
