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
/// This is usually used as fields for structs that derive [`VertexLayout`], which in turn may also
/// derive [`HasAttrib`] that may power up [`Shaper`] API.
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

/// Represents vertex values in a [vertex buffer object](bevy::render::render_resource::Buffer).
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

/// Vertex attribute types that are used in [`Shaper`] API. Common types include [`Pos2dAttrib`],
/// [`ColorAttrib`], and [`UvAttrib`].
pub trait Attrib {
    /// The actual field type this attribute uses.
    type Data: IsAttribData;
}

/// Static marker asserting that a vertex type has a certain attribute, which may benefit [`Shaper`]
/// API.
///
/// # Safety
///
/// Pointer to `Self` offset by `OFFSET` must point to a field whose type is `T::Data`. This is
/// ensured by [`VertexLayout`]'s derive macro.
pub unsafe trait HasAttrib<T: Attrib>: VertexLayout {
    /// The offset of the field representing this attribute.
    const OFFSET: usize;
}

/// 2D position attribute.
pub struct Pos2dAttrib;
impl Attrib for Pos2dAttrib {
    type Data = Vec2;
}

/// 3D position attribute.
pub struct Pos3dAttrib;
impl Attrib for Pos3dAttrib {
    type Data = Vec3;
}

/// Float-color attribute.
pub struct ColorAttrib<const INDEX: usize = 0>;
impl<const INDEX: usize> Attrib for ColorAttrib<INDEX> {
    type Data = LinearRgba;
}

/// Byte-color attribute.
pub struct ByteColorAttrib<const INDEX: usize = 0>;
impl<const INDEX: usize> Attrib for ByteColorAttrib<INDEX> {
    type Data = [Nor<u8>; 4];
}

/// UV-coordinates attribute.
pub struct UvAttrib<const INDEX: usize = 0>;
impl<const INDEX: usize> Attrib for UvAttrib<INDEX> {
    type Data = Vec2;
}

/// Used in [`Shaper::queue`] to create an index array based on an offset.
///
/// This is a work around `impl FnOnce(u32) -> impl Transfer<u32>` not being possible on current
/// stable Rust.
pub trait IndexQueuer {
    /// Creates the index array that starts from `base_offset`.
    fn queue(self, base_offset: u32) -> impl Transfer<u32>;
}

impl<F: FnOnce(u32) -> T, T: Transfer<u32>> IndexQueuer for F {
    #[inline]
    fn queue(self, base_offset: u32) -> impl Transfer<u32> {
        self(base_offset)
    }
}

/// Provides utility methods like positioning, coloring, setting UV coordinates, and more to an
/// array of vertices.
///
/// This utilizes [`HasAttrib`] to determine whether the vertex it's working with supports those
/// attributes.
#[derive(Debug, Copy, Clone)]
pub struct Shaper<T: Vertex, const VERTICES: usize> {
    /// The raw vertex array for manual access.
    pub vertices: [T; VERTICES],
}

impl<T: Vertex, const VERTICES: usize> Default for Shaper<T, VERTICES> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Vertex, const VERTICES: usize> Shaper<T, VERTICES> {
    /// Creates a new shaper.
    #[inline]
    pub fn new() -> Self {
        Self {
            vertices: [T::zeroed(); VERTICES],
        }
    }

    /// Manually sets the vertex at a given index.
    ///
    /// # Panics
    ///
    /// Panics if `index >= VERTICES`.
    #[inline]
    pub fn at(&mut self, index: usize, vertex: T) -> &mut Self {
        self.vertices[index] = vertex;
        self
    }

    /// Sets a vertex attribute for all vertices.
    #[inline]
    pub fn attribs<M: Attrib>(&mut self, attributes: [M::Data; VERTICES]) -> &mut Self
    where T: HasAttrib<M> {
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

    /// Sets a vertex attribute for a specific vertex.
    ///
    /// # Panics
    ///
    /// Panics if `index >= VERTICES`.
    #[inline]
    pub fn attrib_at<M: Attrib>(&mut self, index: usize, attribute: M::Data) -> &mut Self
    where T: HasAttrib<M> {
        let offset = const { <T as HasAttrib<M>>::OFFSET };
        let dst = (&raw mut self.vertices[index]).cast::<u8>();

        unsafe {
            dst.add(offset)
                .copy_from_nonoverlapping((&raw const attribute).cast::<u8>(), size_of::<M::Data>())
        }

        self
    }

    /// Finishes using the [`Shaper`] API.
    #[inline]
    pub fn queue(self, queuer: &impl VertexQueuer<Vertex = T>, layer: f32, key: T::PipelineKey, indices: impl IndexQueuer) {
        queuer.request(layer, key, indices.queue(queuer.data(self.vertices.as_ref())))
    }

    /// Sets 2D positions for all vertices.
    #[inline]
    pub fn pos2d(&mut self, positions: [Vec2; VERTICES]) -> &mut Self
    where T: HasAttrib<Pos2dAttrib> {
        self.attribs::<Pos2dAttrib>(positions)
    }

    /// Sets a 2D position for a vertex.
    ///
    /// # Panics
    ///
    /// Panics if `index >= VERTICES`.
    #[inline]
    pub fn pos2d_at(&mut self, index: usize, position: impl Into<Vec2>) -> &mut Self
    where T: HasAttrib<Pos2dAttrib> {
        self.attrib_at::<Pos2dAttrib>(index, position.into())
    }

    /// Sets 3D positions for all vertices.
    #[inline]
    pub fn pos3d(&mut self, positions: [Vec3; VERTICES]) -> &mut Self
    where T: HasAttrib<Pos3dAttrib> {
        self.attribs::<Pos3dAttrib>(positions)
    }

    /// Sets a 3D position for a vertex.
    ///
    /// # Panics
    ///
    /// Panics if `index >= VERTICES`.
    #[inline]
    pub fn pos3d_at(&mut self, index: usize, position: impl Into<Vec3>) -> &mut Self
    where T: HasAttrib<Pos3dAttrib> {
        self.attrib_at::<Pos3dAttrib>(index, position.into())
    }

    /// Colors all vertices with a uniform color.
    #[inline]
    pub fn color<const INDEX: usize>(&mut self, color: impl Into<LinearRgba>) -> &mut Self
    where T: HasAttrib<ColorAttrib<INDEX>> {
        let color = color.into();
        self.attribs::<ColorAttrib<INDEX>>([color; VERTICES])
    }

    /// Sets colors for all vertices.
    #[inline]
    pub fn colors<const INDEX: usize>(&mut self, colors: [LinearRgba; VERTICES]) -> &mut Self
    where T: HasAttrib<ColorAttrib<INDEX>> {
        self.attribs::<ColorAttrib<INDEX>>(colors)
    }

    /// Sets a color for a vertex.
    ///
    /// # Panics
    ///
    /// Panics if `index >= VERTICES`.
    #[inline]
    pub fn color_at<const INDEX: usize>(&mut self, index: usize, color: impl Into<LinearRgba>) -> &mut Self
    where T: HasAttrib<ColorAttrib<INDEX>> {
        self.attrib_at::<ColorAttrib<INDEX>>(index, color.into())
    }

    /// Colors all vertices with a uniform color.
    #[inline]
    pub fn byte_color<const INDEX: usize>(&mut self, color: [Nor<u8>; 4]) -> &mut Self
    where T: HasAttrib<ByteColorAttrib<INDEX>> {
        let color = color.into();
        self.attribs::<ByteColorAttrib<INDEX>>([color; VERTICES])
    }

    /// Sets colors for all vertices.
    #[inline]
    pub fn byte_colors<const INDEX: usize>(&mut self, colors: [[Nor<u8>; 4]; VERTICES]) -> &mut Self
    where T: HasAttrib<ByteColorAttrib<INDEX>> {
        self.attribs::<ByteColorAttrib<INDEX>>(colors)
    }

    /// Sets a color for a vertex.
    ///
    /// # Panics
    ///
    /// Panics if `index >= VERTICES`.
    #[inline]
    pub fn byte_color_at<const INDEX: usize>(&mut self, index: usize, color: [Nor<u8>; 4]) -> &mut Self
    where T: HasAttrib<ByteColorAttrib<INDEX>> {
        self.attrib_at::<ByteColorAttrib<INDEX>>(index, color.into())
    }

    /// Assigns UV coordinates for all vertices.
    #[inline]
    pub fn uv<const INDEX: usize>(&mut self, positions: [Vec2; VERTICES]) -> &mut Self
    where T: HasAttrib<UvAttrib<INDEX>> {
        self.attribs::<UvAttrib<INDEX>>(positions)
    }

    /// Sets a UV coordinate for a vertex.
    ///
    /// # Panics
    ///
    /// Panics if `index >= VERTICES`.
    #[inline]
    pub fn uv_at<const INDEX: usize>(&mut self, index: usize, position: impl Into<Vec2>) -> &mut Self
    where T: HasAttrib<UvAttrib<INDEX>> {
        self.attrib_at::<UvAttrib<INDEX>>(index, position.into())
    }
}

impl<T: Vertex> Shaper<T, 4> {
    /// Positions the rectangle based on a center position and size.
    ///
    /// This sets the attributes in the order of bottom-left, bottom-right, top-right, and top-left,
    /// which works in tandem with [`Self::queue_rect`].
    #[inline]
    pub fn rect(&mut self, center: impl Into<Vec2>, size: impl Into<Vec2>) -> &mut Self
    where T: HasAttrib<Pos2dAttrib> {
        let Vec2 { x, y } = center.into();
        let Vec2 { x: w, y: h } = size.into() / 2.0;

        self.pos2d([vec2(x - w, y - h), vec2(x + w, y - h), vec2(x + w, y + h), vec2(x - w, y + h)]);
        self
    }

    /// Positions the rectangle based on the bottom-left position and size.
    ///
    /// This sets the attributes in the order of bottom-left, bottom-right, top-right, and top-left,
    /// which works in tandem with [`Self::queue_rect`].
    #[inline]
    pub fn rect_bl(&mut self, bottom_left: impl Into<Vec2>, size: impl Into<Vec2>) -> &mut Self
    where T: HasAttrib<Pos2dAttrib> {
        let Vec2 { x, y } = bottom_left.into();
        let Vec2 { x: w, y: h } = size.into();

        self.pos2d([vec2(x, y), vec2(x + w, y), vec2(x + w, y + h), vec2(x, y + h)])
    }

    /// Assigns UV coordinates based on an atlas sprite entry. Note that this flips the V
    /// coordinate, since images are actually flipped vertically (y=0 is at the top, not bottom).
    ///
    /// This sets the attributes in the order of bottom-left, bottom-right, top-right, and top-left,
    /// which works in tandem with [`Self::queue_rect`].
    #[inline]
    pub fn uv_rect<const INDEX: usize>(&mut self, rect: URect, atlas_size: UVec2) -> &mut Self
    where T: HasAttrib<UvAttrib<INDEX>> {
        let page = atlas_size.as_vec2();
        let Vec2 { x: u, y: v2 } = rect.min.as_vec2() / page;
        let Vec2 { x: u2, y: v } = rect.max.as_vec2() / page;

        self.uv::<INDEX>([vec2(u, v), vec2(u2, v), vec2(u2, v2), vec2(u, v2)])
    }

    /// Convenience method for [`Self::queue`] where the index array is `[0, 1, 2, 2, 3, 0]`, offset
    /// by the base index.
    #[inline]
    pub fn queue_rect(self, queuer: &impl VertexQueuer<Vertex = T>, layer: f32, key: T::PipelineKey) {
        self.queue(queuer, layer, key, |o| [o, o + 1, o + 2, o + 2, o + 3, o])
    }
}
