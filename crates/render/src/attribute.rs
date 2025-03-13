//! Defines functionalitie associated with vertex attributes and their respective layouts and
//! formats.

use std::borrow::{Borrow, BorrowMut};

use bevy_color::prelude::*;
use bevy_derive::{Deref, DerefMut};
use bevy_math::prelude::*;
use bevy_reflect::prelude::*;
use bevy_render::render_resource::{VertexAttribute, VertexFormat};
use bytemuck::{Pod, Zeroable};
pub use hephae_render_derive::VertexLayout;

/// Represents values that, when passed to shaders as vertex attributes, are to be treated as
/// normalized floating point numbers. For example, `[u8; 2]`'s format is [`VertexFormat::Uint8x2`],
/// while `[Nor<u8>; 2]`'s format is [`VertexFormat::Unorm8x2`].
#[derive(Reflect, Debug, Copy, Clone, Default, Pod, Zeroable, PartialEq, Eq, PartialOrd, Ord, Deref, DerefMut)]
#[repr(transparent)]
pub struct Nor<T>(pub T);
impl<T> From<T> for Nor<T> {
    #[inline]
    fn from(value: T) -> Self {
        Self(value)
    }
}

impl<T> AsRef<T> for Nor<T> {
    #[inline]
    fn as_ref(&self) -> &T {
        self
    }
}

impl<T> AsMut<T> for Nor<T> {
    #[inline]
    fn as_mut(&mut self) -> &mut T {
        self
    }
}

impl<T> Borrow<T> for Nor<T> {
    #[inline]
    fn borrow(&self) -> &T {
        self
    }
}

impl<T> BorrowMut<T> for Nor<T> {
    #[inline]
    fn borrow_mut(&mut self) -> &mut T {
        self
    }
}

/// Extension trait for `LinearRgba`.
pub trait LinearRgbaExt {
    /// [`to_u8_array`](LinearRgba::to_u8_array), treated as normalized values. Useful for
    /// byte-color vertex attributes.
    fn to_nor_array(self) -> [Nor<u8>; 4];
}

impl LinearRgbaExt for LinearRgba {
    #[inline]
    fn to_nor_array(self) -> [Nor<u8>; 4] {
        self.to_u8_array().map(Nor)
    }
}

/// Marks the type as acceptable by shader programs as vertex attributes. You shouldn't implement
/// this manually, as this crate already does that for you.
///
/// # Safety
///
/// [`FORMAT::size()`](VertexFormat::size) == [`size_of::<Self>()`](size_of).
pub unsafe trait IsVertexAttribute: Pod {
    /// The associated vertex format of this vertex attribute.
    const FORMAT: VertexFormat;
}

macro_rules! impl_is_vertex_attribute {
    ($($target:ty => $result:ident)*) => {
        $(
            const _: () = assert!(size_of::<$target>() as u64 == VertexFormat::$result.size());

            // Safety: Assertion above guarantees same sizes.
            unsafe impl IsVertexAttribute for $target {
                const FORMAT: VertexFormat = VertexFormat::$result;
            }
        )*
    };
}

impl_is_vertex_attribute! {
    [u8; 2] => Uint8x2
    [u8; 4] => Uint8x4
    [i8; 2] => Sint8x2
    [i8; 4] => Sint8x4
    [Nor<u8>; 2] => Unorm8x2
    [Nor<u8>; 4] => Unorm8x4
    [Nor<i8>; 2] => Snorm8x2
    [Nor<i8>; 4] => Snorm8x4
    [u16; 2] => Uint16x2
    [u16; 4] => Uint16x4
    [i16; 2] => Sint16x2
    [i16; 4] => Sint16x4
    [Nor<u16>; 2] => Unorm16x2
    [Nor<u16>; 4] => Unorm16x4
    [Nor<i16>; 2] => Snorm16x2
    [Nor<i16>; 4] => Snorm16x4
    // Currently, `Float16x2` and `Float16x4` are ignored.
    f32 => Float32
    [f32; 1] => Float32
    [f32; 2] => Float32x2
    [f32; 3] => Float32x3
    [f32; 4] => Float32x4
    u32 => Uint32
    [u32; 1] => Uint32
    [u32; 2] => Uint32x2
    [u32; 3] => Uint32x3
    [u32; 4] => Uint32x4
    i32 => Sint32
    [i32; 1] => Sint32
    [i32; 2] => Sint32x2
    [i32; 3] => Sint32x3
    [i32; 4] => Sint32x4
    f64 => Float64
    [f64; 1] => Float64
    [f64; 2] => Float64x2
    [f64; 3] => Float64x3
    [f64; 4] => Float64x4
    // Currently, `Unorm10_10_10_2` is ignored.
    Vec2 => Float32x2
    Vec3 => Float32x3
    Vec4 => Float32x4
    LinearRgba => Float32x4
}

/// Represents vertex values in a [vertex buffer object](bevy_render::render_resource::Buffer).
///
/// # Safety
///
/// - The sum of `ATTRIBUTES`'s [size](VertexFormat::size) must be equal to
///   [`size_of::<Self>()`](size_of).
/// - Each `ATTRIBUTES`'s [offset](`VertexAttribute::offset`) must be equal to [`offset_of!(Self,
///   field)`](std::mem::offset_of), where `field` is the field represented by the attribute.
pub unsafe trait VertexLayout: Pod {
    /// The attributes of this layout.
    const ATTRIBUTES: &'static [VertexAttribute];
}
