use bevy_ecs::prelude::*;
use bevy_reflect::prelude::*;
use bevy_transform::prelude::*;
use taffy::{BlockContainerStyle, BlockItemStyle, CoreStyle, FlexboxContainerStyle, FlexboxItemStyle};

use crate::node::{ComputedUi, UiCache};

#[derive(Component, Reflect, Clone, Default)]
#[require(Transform, ComputedUi, UiCache)]
#[reflect(Component, Default)]
pub struct Ui {
    pub display: Display,
    pub box_sizing: BoxSizing,
    pub overflow_x: Overflow,
    pub overflow_y: Overflow,
    pub scrollbar_width: f32,
    pub position: Position,
    pub inset: UiBorder,
    pub size: UiSize,
    pub min_size: UiSize,
    pub max_size: UiSize,
    pub aspect_ratio: Option<f32>,
    pub margin: UiBorder,
    pub padding: UiBorder,
    pub border: UiBorder,
    pub flex_direction: FlexDirection,
    pub flex_wrap: FlexWrap,
    pub gap: UiSize,
    pub align_content: AlignContent,
    pub align_items: AlignItems,
    pub justify_content: JustifyContent,
    pub flex_basis: Val,
    pub flex_grow: f32,
    pub flex_shrink: f32,
    pub align_self: AlignSelf,
}

#[derive(Reflect, Copy, Clone, Default)]
#[reflect(Default)]
pub enum Val {
    Abs(f32),
    Rel(f32),
    #[default]
    Auto,
}

impl From<Val> for taffy::Dimension {
    #[inline]
    fn from(value: Val) -> Self {
        match value {
            Val::Abs(abs) => Self::Length(abs),
            Val::Rel(rel) => Self::Percent(rel),
            Val::Auto => Self::Length(0.),
        }
    }
}

impl From<Val> for taffy::LengthPercentage {
    #[inline]
    fn from(value: Val) -> Self {
        match value {
            Val::Abs(abs) => Self::Length(abs),
            Val::Rel(rel) => Self::Percent(rel),
            Val::Auto => Self::Length(0.),
        }
    }
}

impl From<Val> for taffy::LengthPercentageAuto {
    #[inline]
    fn from(value: Val) -> Self {
        match value {
            Val::Abs(abs) => Self::Length(abs),
            Val::Rel(rel) => Self::Percent(rel),
            Val::Auto => Self::Auto,
        }
    }
}

#[derive(Reflect, Copy, Clone, Default)]
#[reflect(Default)]
pub struct UiSize {
    pub width: Val,
    pub height: Val,
}

impl<T: From<Val>> From<UiSize> for taffy::Size<T> {
    #[inline]
    fn from(UiSize { width, height }: UiSize) -> Self {
        Self {
            width: width.into(),
            height: height.into(),
        }
    }
}

#[derive(Reflect, Copy, Clone, Default)]
#[reflect(Default)]
pub struct UiBorder {
    pub left: Val,
    pub right: Val,
    pub bottom: Val,
    pub top: Val,
}

impl<T: From<Val>> From<UiBorder> for taffy::Rect<T> {
    #[inline]
    fn from(
        UiBorder {
            left,
            right,
            bottom,
            top,
        }: UiBorder,
    ) -> Self {
        Self {
            left: left.into(),
            right: right.into(),
            bottom: bottom.into(),
            top: top.into(),
        }
    }
}

#[derive(Reflect, Copy, Clone, Default)]
#[reflect(Default)]
pub enum Display {
    #[default]
    Flexbox,
    Block,
    None,
}

impl From<Display> for taffy::BoxGenerationMode {
    #[inline]
    fn from(value: Display) -> Self {
        match value {
            Display::Flexbox | Display::Block => Self::Normal,
            Display::None => Self::None,
        }
    }
}

/// Specifies whether size styles for this node are assigned to the node's "content box" or "border
/// box".
#[derive(Reflect, Copy, Clone, PartialEq, Eq, Debug, Default)]
#[reflect(Default)]
pub enum BoxSizing {
    /// Size styles such `size`, `min_size`, `max_size` specify the box's "content box" (the size
    /// excluding padding/border/margin).
    #[default]
    BorderBox,
    /// Size styles such `size`, `min_size`, `max_size` specify the box's "border box" (the size
    /// excluding margin but including padding/border).
    ContentBox,
}

impl From<BoxSizing> for taffy::BoxSizing {
    #[inline]
    fn from(value: BoxSizing) -> Self {
        match value {
            BoxSizing::BorderBox => Self::BorderBox,
            BoxSizing::ContentBox => Self::ContentBox,
        }
    }
}

/// How children overflowing their container should affect layout.
#[derive(Reflect, Copy, Clone, PartialEq, Eq, Debug, Default)]
#[reflect(Default)]
pub enum Overflow {
    /// The automatic minimum size of this node as a flexbox/grid item should be based on the size
    /// of its content. Content that overflows this node *should* contribute to the scroll
    /// region of its parent.
    #[default]
    Visible,
    /// The automatic minimum size of this node as a flexbox/grid item should be based on the size
    /// of its content. Content that overflows this node should *not* contribute to the scroll
    /// region of its parent.
    Clip,
    /// The automatic minimum size of this node as a flexbox/grid item should be `0`.
    /// Content that overflows this node should *not* contribute to the scroll region of its parent.
    Hidden,
    /// The automatic minimum size of this node as a flexbox/grid item should be `0`. Additionally,
    /// space should be reserved for a scrollbar. The amount of space reserved is controlled by
    /// the `scrollbar_width` property. Content that overflows this node should *not* contribute
    /// to the scroll region of its parent.
    Scroll,
}

impl From<Overflow> for taffy::Overflow {
    #[inline]
    fn from(value: Overflow) -> Self {
        match value {
            Overflow::Visible => Self::Visible,
            Overflow::Clip => Self::Clip,
            Overflow::Hidden => Self::Hidden,
            Overflow::Scroll => Self::Scroll,
        }
    }
}

/// The positioning strategy for this item.
#[derive(Reflect, Copy, Clone, PartialEq, Eq, Default)]
#[reflect(Default)]
pub enum Position {
    /// The offset is computed relative to the final position given by the layout algorithm.
    /// Offsets do not affect the position of any other items; they are effectively a correction
    /// factor applied at the end.
    #[default]
    Relative,
    /// The offset is computed relative to this item's closest positioned ancestor, if any.
    /// Otherwise, it is placed relative to the origin.
    /// No space is created for the item in the page layout, and its size will not be altered.
    Absolute,
}

impl From<Position> for taffy::Position {
    #[inline]
    fn from(value: Position) -> Self {
        match value {
            Position::Relative => Self::Relative,
            Position::Absolute => Self::Absolute,
        }
    }
}

impl CoreStyle for Ui {
    #[inline]
    fn box_generation_mode(&self) -> taffy::BoxGenerationMode {
        self.display.into()
    }

    #[inline]
    fn is_block(&self) -> bool {
        matches!(self.display, Display::Block)
    }

    #[inline]
    fn box_sizing(&self) -> taffy::BoxSizing {
        self.box_sizing.into()
    }

    #[inline]
    fn overflow(&self) -> taffy::Point<taffy::Overflow> {
        taffy::Point {
            x: self.overflow_x.into(),
            y: self.overflow_y.into(),
        }
    }

    #[inline]
    fn scrollbar_width(&self) -> f32 {
        self.scrollbar_width
    }

    #[inline]
    fn position(&self) -> taffy::Position {
        self.position.into()
    }

    #[inline]
    fn inset(&self) -> taffy::Rect<taffy::LengthPercentageAuto> {
        self.inset.into()
    }

    #[inline]
    fn size(&self) -> taffy::Size<taffy::Dimension> {
        self.size.into()
    }

    #[inline]
    fn min_size(&self) -> taffy::Size<taffy::Dimension> {
        self.min_size.into()
    }

    #[inline]
    fn max_size(&self) -> taffy::Size<taffy::Dimension> {
        self.max_size.into()
    }

    #[inline]
    fn aspect_ratio(&self) -> Option<f32> {
        self.aspect_ratio
    }

    #[inline]
    fn margin(&self) -> taffy::Rect<taffy::LengthPercentageAuto> {
        self.margin.into()
    }

    #[inline]
    fn padding(&self) -> taffy::Rect<taffy::LengthPercentage> {
        self.padding.into()
    }

    #[inline]
    fn border(&self) -> taffy::Rect<taffy::LengthPercentage> {
        self.border.into()
    }
}

/// The direction of the flexbox layout main axis.
#[derive(Reflect, Copy, Clone, PartialEq, Eq, Default)]
#[reflect(Default)]
pub enum FlexDirection {
    /// Items will be added from left to right in a row.
    #[default]
    Row,
    /// Items will be added from top to bottom in a column.
    Column,
    /// Items will be added from right to left in a row.
    RowReverse,
    /// Items will be added from bottom to top in a column.
    ColumnReverse,
}

impl From<FlexDirection> for taffy::FlexDirection {
    #[inline]
    fn from(value: FlexDirection) -> Self {
        match value {
            FlexDirection::Row => Self::Row,
            FlexDirection::Column => Self::Column,
            FlexDirection::RowReverse => Self::RowReverse,
            FlexDirection::ColumnReverse => Self::ColumnReverse,
        }
    }
}

/// Controls whether flex items are forced onto one line or can wrap onto multiple lines.
#[derive(Reflect, Copy, Clone, PartialEq, Eq, Default)]
#[reflect(Default)]
pub enum FlexWrap {
    /// Items will not wrap and stay on a single line,
    #[default]
    NoWrap,
    /// Items will wrap according to this item's [`FlexDirection`],
    Wrap,
    /// Items will wrap in the opposite direction to this item's [`FlexDirection`].
    WrapReverse,
}

impl From<FlexWrap> for taffy::FlexWrap {
    #[inline]
    fn from(value: FlexWrap) -> Self {
        match value {
            FlexWrap::NoWrap => Self::NoWrap,
            FlexWrap::Wrap => Self::Wrap,
            FlexWrap::WrapReverse => Self::WrapReverse,
        }
    }
}

/// Sets the distribution of space between and around content items.
/// For Flexbox it controls alignment in the cross axis.
/// For Grid it controls alignment in the block axis.
#[derive(Reflect, Copy, Clone, PartialEq, Eq, Default)]
#[reflect(Default)]
pub enum AlignContent {
    /// Items are placed as-is.
    #[default]
    None,
    /// Items are packed toward the start of the axis.
    Start,
    /// Items are packed toward the end of the axis.
    End,
    /// Items are packed towards the `flex-relative` start of the axis.
    FlexStart,
    /// Items are packed towards the `flex-relative` end of the axis.
    FlexEnd,
    /// Items are centered around the middle of the axis.
    Center,
    /// Items are stretched to fill the container.
    Stretch,
    /// The first and last items are aligned flush with the edges of the container (no gap)
    /// The gap between items is distributed evenly.
    SpaceBetween,
    /// The gap between the first and last items is exactly **the same** as the gap between items.
    /// The gaps are distributed evenly
    SpaceEvenly,
    /// The gap between the first and last items is exactly **half** the gap between items.
    /// The gaps are distributed evenly in proportion to these ratios.
    SpaceAround,
}

impl From<AlignContent> for Option<taffy::AlignContent> {
    #[inline]
    fn from(value: AlignContent) -> Self {
        match value {
            AlignContent::None => None,
            AlignContent::Start => Some(taffy::AlignContent::Start),
            AlignContent::End => Some(taffy::AlignContent::End),
            AlignContent::FlexStart => Some(taffy::AlignContent::FlexStart),
            AlignContent::FlexEnd => Some(taffy::AlignContent::FlexEnd),
            AlignContent::Center => Some(taffy::AlignContent::Center),
            AlignContent::Stretch => Some(taffy::AlignContent::Stretch),
            AlignContent::SpaceBetween => Some(taffy::AlignContent::SpaceBetween),
            AlignContent::SpaceEvenly => Some(taffy::AlignContent::SpaceEvenly),
            AlignContent::SpaceAround => Some(taffy::AlignContent::SpaceAround),
        }
    }
}

/// Used to control how child nodes are aligned.
/// For Flexbox it controls alignment in the cross axis.
/// For Grid it controls alignment in the block axis.
#[derive(Reflect, Copy, Clone, PartialEq, Eq, Default)]
#[reflect(Default)]
pub enum AlignItems {
    /// Items are placed as-is.
    #[default]
    None,
    /// Items are packed toward the start of the axis.
    Start,
    /// Items are packed toward the end of the axis.
    End,
    /// Items are packed towards the flex-relative start of the axis.
    FlexStart,
    /// Items are packed towards the flex-relative end of the axis.
    FlexEnd,
    /// Items are packed along the center of the cross axis.
    Center,
    /// Items are aligned such as their baselines align.
    Baseline,
    /// Stretch to fill the container.
    Stretch,
}

impl From<AlignItems> for Option<taffy::AlignItems> {
    #[inline]
    fn from(value: AlignItems) -> Self {
        match value {
            AlignItems::None => None,
            AlignItems::Start => Some(taffy::AlignItems::Start),
            AlignItems::End => Some(taffy::AlignItems::End),
            AlignItems::FlexStart => Some(taffy::AlignItems::FlexStart),
            AlignItems::FlexEnd => Some(taffy::AlignItems::FlexEnd),
            AlignItems::Center => Some(taffy::AlignItems::Center),
            AlignItems::Baseline => Some(taffy::AlignItems::Baseline),
            AlignItems::Stretch => Some(taffy::AlignItems::Stretch),
        }
    }
}

/// Sets the distribution of space between and around content items.
/// For Flexbox it controls alignment in the main axis.
/// For Grid it controls alignment in the inline axis.
pub type JustifyContent = AlignContent;

impl FlexboxContainerStyle for Ui {
    #[inline]
    fn flex_direction(&self) -> taffy::FlexDirection {
        self.flex_direction.into()
    }

    #[inline]
    fn flex_wrap(&self) -> taffy::FlexWrap {
        self.flex_wrap.into()
    }

    #[inline]
    fn gap(&self) -> taffy::Size<taffy::LengthPercentage> {
        self.gap.into()
    }

    #[inline]
    fn align_content(&self) -> Option<taffy::AlignContent> {
        self.align_content.into()
    }

    #[inline]
    fn align_items(&self) -> Option<taffy::AlignItems> {
        self.align_items.into()
    }

    #[inline]
    fn justify_content(&self) -> Option<taffy::JustifyContent> {
        self.justify_content.into()
    }
}

/// Controls alignment of an individual node.
pub type AlignSelf = AlignItems;

impl FlexboxItemStyle for Ui {
    #[inline]
    fn flex_basis(&self) -> taffy::Dimension {
        self.flex_basis.into()
    }

    #[inline]
    fn flex_grow(&self) -> f32 {
        self.flex_grow
    }

    #[inline]
    fn flex_shrink(&self) -> f32 {
        self.flex_shrink
    }

    #[inline]
    fn align_self(&self) -> Option<taffy::AlignSelf> {
        self.align_self.into()
    }
}

impl BlockContainerStyle for Ui {
    #[inline]
    fn text_align(&self) -> taffy::TextAlign {
        taffy::TextAlign::Auto
    }
}

impl BlockItemStyle for Ui {
    #[inline]
    fn is_table(&self) -> bool {
        false
    }
}
