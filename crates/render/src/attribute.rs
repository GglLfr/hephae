//! Defines functionalities associated with vertex attributes and their respective layouts and
//! formats.

use std::{
    borrow::{Borrow, BorrowMut},
    ptr::addr_eq,
};

use bevy::{
    prelude::*,
    render::render_resource::{VertexAttribute, VertexFormat},
};
use bytemuck::{Pod, Zeroable};
pub use hephae_render_derive::VertexLayout;
use vec_belt::Transfer;

use crate::{drawer::VertexQueuer, vertex::Vertex};

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
pub unsafe trait IsAttribData: Pod {
    /// The associated vertex format of this vertex attribute.
    const FORMAT: VertexFormat;
}

macro_rules! impl_is_attrib_data {
    ($($target:ty => $result:ident)*) => {
        $(
            const _: () = assert!(size_of::<$target>() as u64 == VertexFormat::$result.size());

            // Safety: Assertion above guarantees same sizes.
            unsafe impl IsAttribData for $target {
                const FORMAT: VertexFormat = VertexFormat::$result;
            }
        )*
    };
}

impl_is_attrib_data! {
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

pub trait Attrib {
    type Data: IsAttribData;
}

pub unsafe trait HasAttrib<T: Attrib>: VertexLayout {
    const OFFSET: usize;
}

pub struct Pos2dAttrib;
impl Attrib for Pos2dAttrib {
    type Data = Vec2;
}

pub struct Pos3dAttrib;
impl Attrib for Pos3dAttrib {
    type Data = Vec3;
}

pub struct ColorAttrib;
impl Attrib for ColorAttrib {
    type Data = LinearRgba;
}

pub struct ByteColorAttrib;
impl Attrib for ByteColorAttrib {
    type Data = [Nor<u8>; 4];
}

pub struct UvAttrib;
impl Attrib for UvAttrib {
    type Data = Vec2;
}

pub trait IndexQueuer {
    fn queue(self, base_offset: u32) -> impl Transfer<u32>;
}

impl<F: FnOnce(u32) -> T, T: Transfer<u32>> IndexQueuer for F {
    #[inline]
    fn queue(self, base_offset: u32) -> impl Transfer<u32> {
        self(base_offset)
    }
}

#[derive(Debug, Copy, Clone)]
pub struct Shaper<T: Vertex, const VERTICES: usize> {
    pub vertices: [T; VERTICES],
}

unsafe impl<T: Vertex, const VERTICES: usize> Transfer<T> for Shaper<T, VERTICES> {
    #[inline]
    fn len(&self) -> usize {
        VERTICES
    }

    #[inline]
    unsafe fn transfer(self, len: usize, dst: *mut T) {
        unsafe { self.vertices.transfer(len, dst) }
    }
}

impl<T: Vertex, const VERTICES: usize> Shaper<T, VERTICES> {
    #[inline]
    pub fn new() -> Self {
        Self {
            vertices: [T::zeroed(); VERTICES],
        }
    }

    #[inline]
    pub fn at(&mut self, index: usize, vertex: T) -> &mut Self {
        self.vertices[index] = vertex;
        self
    }

    #[inline]
    pub fn attribs<M: Attrib>(&mut self, attributes: [M::Data; VERTICES]) -> &mut Self
    where
        T: HasAttrib<M>,
    {
        let offset = const { <T as HasAttrib<M>>::OFFSET };
        let mut src = attributes.as_ptr().cast::<u8>();
        let mut dst = self.vertices.as_mut_ptr().cast::<u8>();

        unsafe {
            let end = dst.add(VERTICES * size_of::<T>());
            while !addr_eq(dst, end) {
                dst.add(offset).copy_from_nonoverlapping(src, size_of::<M::Data>());

                src = src.add(size_of::<M::Data>());
                dst = dst.add(size_of::<T>());
            }
        }

        self
    }

    #[inline]
    pub fn attrib_at<M: Attrib>(&mut self, index: usize, attribute: M::Data) -> &mut Self
    where
        T: HasAttrib<M>,
    {
        let offset = const { <T as HasAttrib<M>>::OFFSET };
        let dst = (&raw mut self.vertices[index]).cast::<u8>();

        unsafe {
            dst.add(offset)
                .copy_from_nonoverlapping((&raw const attribute).cast::<u8>(), size_of::<M::Data>())
        }

        self
    }

    #[inline]
    pub fn queue(self, queuer: &impl VertexQueuer<Vertex = T>, layer: f32, key: T::PipelineKey, indices: impl IndexQueuer) {
        queuer.request(layer, key, indices.queue(queuer.data(self.vertices)))
    }
}

impl<T: Vertex + HasAttrib<Pos2dAttrib>, const VERTICES: usize> Shaper<T, VERTICES> {
    #[inline]
    pub fn pos2d(&mut self, positions: [Vec2; VERTICES]) -> &mut Self {
        self.attribs::<Pos2dAttrib>(positions)
    }

    #[inline]
    pub fn pos2d_at(&mut self, index: usize, position: impl Into<Vec2>) -> &mut Self {
        self.attrib_at::<Pos2dAttrib>(index, position.into())
    }
}

impl<T: Vertex + HasAttrib<Pos3dAttrib>, const VERTICES: usize> Shaper<T, VERTICES> {
    #[inline]
    pub fn pos3d(&mut self, positions: [Vec3; VERTICES]) -> &mut Self {
        self.attribs::<Pos3dAttrib>(positions)
    }

    #[inline]
    pub fn pos3d_at(&mut self, index: usize, position: impl Into<Vec3>) -> &mut Self {
        self.attrib_at::<Pos3dAttrib>(index, position.into())
    }
}

impl<T: Vertex + HasAttrib<ColorAttrib>, const VERTICES: usize> Shaper<T, VERTICES> {
    #[inline]
    pub fn color(&mut self, color: impl Into<LinearRgba>) -> &mut Self {
        let color = color.into();
        self.attribs::<ColorAttrib>([color; VERTICES])
    }

    #[inline]
    pub fn colors(&mut self, colors: [LinearRgba; VERTICES]) -> &mut Self {
        self.attribs::<ColorAttrib>(colors)
    }

    #[inline]
    pub fn color_at(&mut self, index: usize, color: impl Into<LinearRgba>) -> &mut Self {
        self.attrib_at::<ColorAttrib>(index, color.into())
    }
}

impl<T: Vertex + HasAttrib<ByteColorAttrib>, const VERTICES: usize> Shaper<T, VERTICES> {
    #[inline]
    pub fn byte_color(&mut self, color: [Nor<u8>; 4]) -> &mut Self {
        self.attribs::<ByteColorAttrib>([color; VERTICES])
    }

    #[inline]
    pub fn byte_colors(&mut self, colors: [[Nor<u8>; 4]; VERTICES]) -> &mut Self {
        self.attribs::<ByteColorAttrib>(colors)
    }

    #[inline]
    pub fn byte_color_at(&mut self, index: usize, color: [Nor<u8>; 4]) -> &mut Self {
        self.attrib_at::<ByteColorAttrib>(index, color.into())
    }
}

impl<T: Vertex + HasAttrib<UvAttrib>, const VERTICES: usize> Shaper<T, VERTICES> {
    #[inline]
    pub fn uv(&mut self, positions: [Vec2; VERTICES]) -> &mut Self {
        self.attribs::<UvAttrib>(positions)
    }

    #[inline]
    pub fn uv_at(&mut self, index: usize, position: impl Into<Vec2>) -> &mut Self {
        self.attrib_at::<UvAttrib>(index, position.into())
    }
}

impl<T: Vertex> Shaper<T, 4> {
    #[inline]
    pub fn queue_rect(self, queuer: &impl VertexQueuer<Vertex = T>, layer: f32, key: T::PipelineKey) {
        self.queue(queuer, layer, key, |o| [o, o + 1, o + 2, o + 2, o + 3, o])
    }
}

impl<T: Vertex + HasAttrib<Pos2dAttrib>> Shaper<T, 4> {
    #[inline]
    pub fn rect(&mut self, center: impl Into<Vec2>, size: impl Into<Vec2>) -> &mut Self {
        let Vec2 { x, y } = center.into();
        let Vec2 { x: w, y: h } = size.into() / 2.0;

        self.pos2d([vec2(x - w, y - h), vec2(x + w, y - h), vec2(x + w, y + h), vec2(x - w, y + h)]);
        self
    }

    #[inline]
    pub fn rect_bl(&mut self, bottom_left: impl Into<Vec2>, size: impl Into<Vec2>) -> &mut Self {
        let Vec2 { x, y } = bottom_left.into();
        let Vec2 { x: w, y: h } = size.into();

        self.pos2d([vec2(x, y), vec2(x + w, y), vec2(x + w, y + h), vec2(x, y + h)]);
        self
    }
}

impl<T: Vertex + HasAttrib<UvAttrib>> Shaper<T, 4> {
    #[inline]
    pub fn uv_rect(&mut self, rect: URect, atlas_size: UVec2) -> &mut Self {
        let page = atlas_size.as_vec2();
        let Vec2 { x: u, y: v2 } = rect.min.as_vec2() / page;
        let Vec2 { x: u2, y: v } = rect.max.as_vec2() / page;

        self.uv([vec2(u, v), vec2(u2, v), vec2(u2, v2), vec2(u, v2)]);
        self
    }
}
