//! Defines everything necessary to style a [`Ui`] node.

use bevy::prelude::*;
use taffy::{BlockContainerStyle, BlockItemStyle, CoreStyle, FlexboxContainerStyle, FlexboxItemStyle};

use crate::node::{ComputedUi, UiCaches};

/// A [`Ui`] node, complete with its styling information.
#[derive(Component, Reflect, Clone)]
#[require(Transform, ComputedUi)]
#[reflect(Component, Default)]
pub struct Ui {
    /// Layout strategy to be used when laying out this node.
    pub display: Display,
    /// Defines whether size styles apply to the content box or the border box of the node.
    pub box_sizing: BoxSizing,
    /// How children overflowing their container should affect layout in the X axis.
    pub overflow_x: Overflow,
    /// How children overflowing their container should affect layout in the Y axis.
    pub overflow_y: Overflow,
    /// How much space (in pixels) should be reserved for the scrollbars of [`Overflow::Scroll`].
    pub scrollbar_width: f32,
    /// Determines what the `inset` value use as a base offset.
    pub position: Position,
    /// Determines how the position of this element should be tweaked relative to the layout
    /// defined.
    pub inset: UiBorder,
    /// Sets the initial size of the node.
    pub size: UiSize,
    /// Controls the minimum size of the node.
    pub min_size: UiSize,
    /// Controls the maximum size of the node.
    pub max_size: UiSize,
    /// Sets the preferred aspect ratio for the node, calculated as width divided by height.
    pub aspect_ratio: Option<f32>,
    /// How large the margin should be on each side.
    pub margin: UiBorder,
    /// How large the padding should be on each side.
    pub padding: UiBorder,
    /// How large the border should be on each side.
    pub border: UiBorder,
    /// Defines which direction the main axis flows in.
    pub flex_direction: FlexDirection,
    /// Defines wrapping behavior for when children nodes exhaust their available space.
    pub flex_wrap: FlexWrap,
    /// Determines ow large the gaps between nodes. in the container should be.
    pub gap: UiSize,
    /// Determines how content contained within this node should be aligned in the cross/block axis.
    pub align_content: AlignContent,
    /// Determines how this node's children should be aligned in the cross/block axis.
    pub align_items: AlignItems,
    /// Determines how content contained within this node should be aligned in the main/inline axis.
    pub justify_content: JustifyContent,
    /// Sets the initial main axis size of the node.
    pub flex_basis: Val,
    /// The relative rate at which this node grows when it is expanding to fill space.
    pub flex_grow: f32,
    /// The relative rate at which this node shrinks when it is contracting to fit into space.
    pub flex_shrink: f32,
    /// Determines how this node should be aligned in the cross/block axis, falling back to the
    /// parent's [`AlignItems`] if not set.
    pub align_self: AlignSelf,
}

impl Ui {
    /// The default value.
    pub const DEFAULT: Self = Self {
        display: Display::DEFAULT,
        box_sizing: BoxSizing::DEFAULT,
        overflow_x: Overflow::DEFAULT,
        overflow_y: Overflow::DEFAULT,
        scrollbar_width: 0.,
        position: Position::DEFAULT,
        inset: UiBorder::DEFAULT,
        size: UiSize::DEFAULT,
        min_size: UiSize::DEFAULT,
        max_size: UiSize::DEFAULT,
        aspect_ratio: None,
        margin: UiBorder::all(Val::Abs(0.)),
        padding: UiBorder::all(Val::Abs(0.)),
        border: UiBorder::all(Val::Abs(0.)),
        flex_direction: FlexDirection::DEFAULT,
        flex_wrap: FlexWrap::DEFAULT,
        gap: UiSize::all(Val::Abs(0.)),
        align_content: AlignContent::DEFAULT,
        align_items: AlignItems::DEFAULT,
        justify_content: JustifyContent::DEFAULT,
        flex_basis: Val::DEFAULT,
        flex_grow: 0.,
        flex_shrink: 1.,
        align_self: AlignSelf::DEFAULT,
    };
}

impl Default for Ui {
    #[inline]
    fn default() -> Self {
        Self::DEFAULT
    }
}

pub(crate) fn ui_changed(query: Query<Entity, Changed<Ui>>, mut caches: UiCaches) {
    for e in &query {
        caches.invalidate(e)
    }
}

#[derive(Copy, Clone)]
pub(crate) struct WithCtx<T> {
    pub width: f32,
    pub height: f32,
    pub item: T,
}

/// UI dimension values.
#[derive(Reflect, Copy, Clone)]
#[reflect(Default)]
pub enum Val {
    /// Absolute pixel units.
    Abs(f32),
    /// Ratio of parent's units.
    Rel(f32),
    /// Ratio of viewport width.
    Vw(f32),
    /// Ratio of viewport height.
    Vh(f32),
    /// Should be automatically computed.
    Auto,
}

impl Val {
    /// The default value for [`Val`].
    pub const DEFAULT: Self = Val::Auto;
}

impl Default for Val {
    #[inline]
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl From<WithCtx<Val>> for taffy::Dimension {
    #[inline]
    fn from(WithCtx { width, height, item }: WithCtx<Val>) -> Self {
        match item {
            Val::Abs(abs) => Self::length(abs),
            Val::Rel(rel) => Self::percent(rel),
            Val::Vw(w) => Self::length(width * w),
            Val::Vh(h) => Self::length(height * h),
            Val::Auto => Self::auto(),
        }
    }
}

impl From<WithCtx<Val>> for taffy::LengthPercentage {
    #[inline]
    fn from(WithCtx { width, height, item }: WithCtx<Val>) -> Self {
        match item {
            Val::Abs(abs) => Self::length(abs),
            Val::Rel(rel) => Self::percent(rel),
            Val::Vw(w) => Self::length(width * w),
            Val::Vh(h) => Self::length(height * h),
            Val::Auto => Self::length(0.),
        }
    }
}

impl From<WithCtx<Val>> for taffy::LengthPercentageAuto {
    #[inline]
    fn from(WithCtx { width, height, item }: WithCtx<Val>) -> Self {
        match item {
            Val::Abs(abs) => Self::length(abs),
            Val::Rel(rel) => Self::percent(rel),
            Val::Vw(w) => Self::length(width * w),
            Val::Vh(h) => Self::length(height * h),
            Val::Auto => Self::auto(),
        }
    }
}

/// A two-dimensional [`Val`], defining the area of a rectangle.
#[derive(Reflect, Copy, Clone)]
#[reflect(Default)]
pub struct UiSize {
    /// The width of the area.
    pub width: Val,
    /// The height of the area.
    pub height: Val,
}

impl UiSize {
    /// The default value for [`UiSize`].
    pub const DEFAULT: Self = Self {
        width: Val::DEFAULT,
        height: Val::DEFAULT,
    };
}

impl Default for UiSize {
    #[inline]
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl UiSize {
    /// Creates a new [`UiSize`].
    #[inline]
    pub const fn new(width: Val, height: Val) -> Self {
        Self { width, height }
    }

    /// Creates a new [`UiSize`] that has the same values for all axes.
    #[inline]
    pub const fn all(value: Val) -> Self {
        Self::new(value, value)
    }

    /// Creates a new [`UiSize`] that has all [absolute](Val::Abs) values.
    #[inline]
    pub const fn abs(width: f32, height: f32) -> Self {
        Self::new(Val::Abs(width), Val::Abs(height))
    }

    /// Creates a new [`UiSize`] that has all [relative](Val::Rel) values.
    #[inline]
    pub const fn rel(width: f32, height: f32) -> Self {
        Self::new(Val::Rel(width), Val::Rel(height))
    }
}

impl<T: From<WithCtx<Val>>> From<WithCtx<UiSize>> for taffy::Size<T> {
    #[inline]
    fn from(WithCtx { width, height, item }: WithCtx<UiSize>) -> Self {
        Self {
            width: WithCtx {
                width,
                height,
                item: item.width,
            }
            .into(),
            height: WithCtx {
                width,
                height,
                item: item.height,
            }
            .into(),
        }
    }
}

/// A rectangle defined by its borders.
#[derive(Reflect, Copy, Clone)]
#[reflect(Default)]
pub struct UiBorder {
    /// The left offset or position of this border.
    pub left: Val,
    /// The right offset or position of this border.
    pub right: Val,
    /// The bottom offset or position of this border.
    pub bottom: Val,
    /// The top offset or position of this border.
    pub top: Val,
}

impl UiBorder {
    /// The default value of [`UiBorder`].
    pub const DEFAULT: Self = Self {
        left: Val::DEFAULT,
        right: Val::DEFAULT,
        bottom: Val::DEFAULT,
        top: Val::DEFAULT,
    };

    /// Creates a new [`UiBorder`].
    #[inline]
    pub const fn new(left: Val, right: Val, bottom: Val, top: Val) -> Self {
        Self {
            left,
            right,
            bottom,
            top,
        }
    }

    /// Creates a new [`UiBorder`] that has the same values for all sides.
    #[inline]
    pub const fn all(value: Val) -> Self {
        Self::new(value, value, value, value)
    }
}

impl Default for UiBorder {
    #[inline]
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl<T: From<WithCtx<Val>>> From<WithCtx<UiBorder>> for taffy::Rect<T> {
    #[inline]
    fn from(WithCtx { width, height, item }: WithCtx<UiBorder>) -> Self {
        let UiBorder {
            left,
            right,
            bottom,
            top,
        } = item;

        Self {
            left: WithCtx {
                width,
                height,
                item: left,
            }
            .into(),
            right: WithCtx {
                width,
                height,
                item: right,
            }
            .into(),
            bottom: WithCtx {
                width,
                height,
                item: bottom,
            }
            .into(),
            top: WithCtx {
                width,
                height,
                item: top,
            }
            .into(),
        }
    }
}

/// Layout strategy to be used when laying out this node.
#[derive(Reflect, Copy, Clone)]
#[reflect(Default)]
pub enum Display {
    /// Use `flex` layout.
    Flexbox,
    /// Use `block` layout.
    Block,
    /// Collapse this node recursively, making it look like it doesn't exist at all.
    None,
}

impl Display {
    /// The default value of [`Display`].
    pub const DEFAULT: Self = Display::Flexbox;
}

impl Default for Display {
    #[inline]
    fn default() -> Self {
        Self::DEFAULT
    }
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
#[derive(Reflect, Copy, Clone, PartialEq, Eq, Debug)]
#[reflect(Default)]
pub enum BoxSizing {
    /// Size styles such `size`, `min_size`, `max_size` specify the box's "content box" (the size
    /// excluding padding/border/margin).
    BorderBox,
    /// Size styles such `size`, `min_size`, `max_size` specify the box's "border box" (the size
    /// excluding margin but including padding/border).
    ContentBox,
}

impl BoxSizing {
    /// The default value of [`BoxSizing`].
    pub const DEFAULT: Self = Self::BorderBox;
}

impl Default for BoxSizing {
    #[inline]
    fn default() -> Self {
        Self::DEFAULT
    }
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
#[derive(Reflect, Copy, Clone, PartialEq, Eq, Debug)]
#[reflect(Default)]
pub enum Overflow {
    /// The automatic minimum size of this node as a flexbox/grid item should be based on the size
    /// of its content. Content that overflows this node *should* contribute to the scroll
    /// region of its parent.
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

impl Overflow {
    /// The default value of [`Overflow`].
    pub const DEFAULT: Self = Self::Visible;
}

impl Default for Overflow {
    #[inline]
    fn default() -> Self {
        Self::DEFAULT
    }
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
#[derive(Reflect, Copy, Clone, PartialEq, Eq)]
#[reflect(Default)]
pub enum Position {
    /// The offset is computed relative to the final position given by the layout algorithm.
    /// Offsets do not affect the position of any other items; they are effectively a correction
    /// factor applied at the end.
    Relative,
    /// The offset is computed relative to this item's closest positioned ancestor, if any.
    /// Otherwise, it is placed relative to the origin.
    /// No space is created for the item in the page layout, and its size will not be altered.
    Absolute,
}

impl Position {
    /// The default value of [`Position`].
    pub const DEFAULT: Self = Self::Relative;
}

impl Default for Position {
    #[inline]
    fn default() -> Self {
        Self::DEFAULT
    }
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

impl CoreStyle for WithCtx<&Ui> {
    #[inline]
    fn box_generation_mode(&self) -> taffy::BoxGenerationMode {
        self.item.display.into()
    }

    #[inline]
    fn is_block(&self) -> bool {
        matches!(self.item.display, Display::Block)
    }

    #[inline]
    fn box_sizing(&self) -> taffy::BoxSizing {
        self.item.box_sizing.into()
    }

    #[inline]
    fn overflow(&self) -> taffy::Point<taffy::Overflow> {
        taffy::Point {
            x: self.item.overflow_y.into(),
            y: self.item.overflow_y.into(),
        }
    }

    #[inline]
    fn scrollbar_width(&self) -> f32 {
        self.item.scrollbar_width
    }

    #[inline]
    fn position(&self) -> taffy::Position {
        self.item.position.into()
    }

    #[inline]
    fn inset(&self) -> taffy::Rect<taffy::LengthPercentageAuto> {
        WithCtx {
            width: self.width,
            height: self.height,
            item: self.item.inset,
        }
        .into()
    }

    #[inline]
    fn size(&self) -> taffy::Size<taffy::Dimension> {
        WithCtx {
            width: self.width,
            height: self.height,
            item: self.item.size,
        }
        .into()
    }

    #[inline]
    fn min_size(&self) -> taffy::Size<taffy::Dimension> {
        WithCtx {
            width: self.width,
            height: self.height,
            item: self.item.min_size,
        }
        .into()
    }

    #[inline]
    fn max_size(&self) -> taffy::Size<taffy::Dimension> {
        WithCtx {
            width: self.width,
            height: self.height,
            item: self.item.max_size,
        }
        .into()
    }

    #[inline]
    fn aspect_ratio(&self) -> Option<f32> {
        self.item.aspect_ratio
    }

    #[inline]
    fn margin(&self) -> taffy::Rect<taffy::LengthPercentageAuto> {
        WithCtx {
            width: self.width,
            height: self.height,
            item: self.item.margin,
        }
        .into()
    }

    #[inline]
    fn padding(&self) -> taffy::Rect<taffy::LengthPercentage> {
        WithCtx {
            width: self.width,
            height: self.height,
            item: self.item.padding,
        }
        .into()
    }

    #[inline]
    fn border(&self) -> taffy::Rect<taffy::LengthPercentage> {
        WithCtx {
            width: self.width,
            height: self.height,
            item: self.item.border,
        }
        .into()
    }
}

/// The direction of the flexbox layout main axis.
#[derive(Reflect, Copy, Clone, PartialEq, Eq)]
#[reflect(Default)]
pub enum FlexDirection {
    /// Items will be added from left to right in a row.
    Row,
    /// Items will be added from top to bottom in a column.
    Column,
    /// Items will be added from right to left in a row.
    RowReverse,
    /// Items will be added from bottom to top in a column.
    ColumnReverse,
}

impl FlexDirection {
    /// The default value of [`FlexDirection`].
    pub const DEFAULT: Self = Self::Row;
}

impl Default for FlexDirection {
    #[inline]
    fn default() -> Self {
        Self::DEFAULT
    }
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
#[derive(Reflect, Copy, Clone, PartialEq, Eq)]
#[reflect(Default)]
pub enum FlexWrap {
    /// Items will not wrap and stay on a single line,
    NoWrap,
    /// Items will wrap according to this item's [`FlexDirection`],
    Wrap,
    /// Items will wrap in the opposite direction to this item's [`FlexDirection`].
    WrapReverse,
}

impl FlexWrap {
    /// The default value of [`FlexWrap`].
    pub const DEFAULT: Self = Self::NoWrap;
}

impl Default for FlexWrap {
    #[inline]
    fn default() -> Self {
        Self::DEFAULT
    }
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
#[derive(Reflect, Copy, Clone, PartialEq, Eq)]
#[reflect(Default)]
pub enum AlignContent {
    /// Items are placed as-is.
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

impl AlignContent {
    /// The default value of [`AlignContent`].
    pub const DEFAULT: Self = Self::None;
}

impl Default for AlignContent {
    #[inline]
    fn default() -> Self {
        Self::None
    }
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
#[derive(Reflect, Copy, Clone, PartialEq, Eq)]
#[reflect(Default)]
pub enum AlignItems {
    /// Items are placed as-is.
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

impl AlignItems {
    /// The default value of [`AlignItems`].
    pub const DEFAULT: Self = Self::None;
}

impl Default for AlignItems {
    #[inline]
    fn default() -> Self {
        Self::DEFAULT
    }
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

impl FlexboxContainerStyle for WithCtx<&Ui> {
    #[inline]
    fn flex_direction(&self) -> taffy::FlexDirection {
        self.item.flex_direction.into()
    }

    #[inline]
    fn flex_wrap(&self) -> taffy::FlexWrap {
        self.item.flex_wrap.into()
    }

    #[inline]
    fn gap(&self) -> taffy::Size<taffy::LengthPercentage> {
        WithCtx {
            width: self.width,
            height: self.height,
            item: self.item.gap,
        }
        .into()
    }

    #[inline]
    fn align_content(&self) -> Option<taffy::AlignContent> {
        self.item.align_content.into()
    }

    #[inline]
    fn align_items(&self) -> Option<taffy::AlignItems> {
        self.item.align_items.into()
    }

    #[inline]
    fn justify_content(&self) -> Option<taffy::JustifyContent> {
        self.item.justify_content.into()
    }
}

/// Controls alignment of an individual node.
pub type AlignSelf = AlignItems;

impl FlexboxItemStyle for WithCtx<&Ui> {
    #[inline]
    fn flex_basis(&self) -> taffy::Dimension {
        WithCtx {
            width: self.width,
            height: self.height,
            item: self.item.flex_basis,
        }
        .into()
    }

    #[inline]
    fn flex_grow(&self) -> f32 {
        self.item.flex_grow
    }

    #[inline]
    fn flex_shrink(&self) -> f32 {
        self.item.flex_shrink
    }

    #[inline]
    fn align_self(&self) -> Option<taffy::AlignSelf> {
        self.item.align_self.into()
    }
}

impl BlockContainerStyle for WithCtx<&Ui> {
    #[inline]
    fn text_align(&self) -> taffy::TextAlign {
        taffy::TextAlign::Auto
    }
}

impl BlockItemStyle for WithCtx<&Ui> {
    #[inline]
    fn is_table(&self) -> bool {
        false
    }
}
