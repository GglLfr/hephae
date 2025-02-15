//! Defines base drawers that work with vertices and supply various vertex commands.

use std::{marker::PhantomData, sync::PoisonError};

use bevy_ecs::{
    prelude::*,
    query::{QueryFilter, QueryItem, ReadOnlyQueryData},
    system::{ReadOnlySystemParam, StaticSystemParam, SystemParamItem},
};
use bevy_reflect::prelude::*;
use bevy_render::{
    self,
    prelude::*,
    sync_world::RenderEntity,
    view::{ExtractedView, RenderVisibleEntities},
    Extract,
};
use fixedbitset::FixedBitSet;
use vec_belt::Transfer;

use crate::{
    pipeline::{DrawBuffers, VisibleDrawers},
    vertex::{DrawItems, Vertex},
};

/// A render world [`Component`] extracted from the main world that will be used to issue draw
/// requests.
pub trait Drawer: TypePath + Component + Sized {
    /// The type of vertex this drawer works with.
    type Vertex: Vertex;

    /// System parameter to fetch when extracting data from the main world.
    type ExtractParam: ReadOnlySystemParam;
    /// Query item to fetch from entities when extracting from those entities to the render world.
    type ExtractData: ReadOnlyQueryData;
    /// Additional query filters accompanying [`ExtractData`](Drawer::ExtractData).
    type ExtractFilter: QueryFilter;

    /// System parameter to fetch when issuing draw requests.
    type DrawParam: ReadOnlySystemParam;

    /// Extracts an instance of this drawer from matching entities, if available.
    fn extract(
        drawer: DrawerExtract<Self>,
        param: &SystemParamItem<Self::ExtractParam>,
        query: QueryItem<Self::ExtractData>,
    );

    /// Issues vertex data and draw requests for the data.
    fn draw(&mut self, param: &SystemParamItem<Self::DrawParam>, queuer: &impl VertexQueuer<Vertex = Self::Vertex>);
}

/// Specifies the behavior of [`Drawer::extract`].
pub enum DrawerExtract<'a, T: Drawer> {
    /// The render-world component exists, and may be used to optimize allocations.
    Borrowed(&'a mut T),
    /// The drawer needs to create a new instance of itself.
    Spawn(&'a mut Option<T>),
}

impl<T: Drawer> DrawerExtract<'_, T> {
    /// Gets a mutable reference to the underlying component, creating a new one if necessary.
    #[inline]
    pub fn get_mut(&mut self, new: impl FnOnce() -> T) -> &mut T {
        match self {
            Self::Borrowed(value) => value,
            Self::Spawn(opt) => opt.insert(new()),
        }
    }

    /// Gets a mutable reference to the underlying component, creating a new one if necessary.
    #[inline]
    pub fn get_or_default(&mut self) -> &mut T
    where
        T: Default,
    {
        self.get_mut(T::default)
    }
}

/// Similar to [`Extend`], except it works with both vertex and index buffers.
pub trait VertexQueuer {
    /// The type of vertex this queuer works with.
    type Vertex: Vertex;

    /// Extends the vertex buffer with the supplied iterator. The returned index should be used as
    /// offset adder to indices passed to [`request`](VertexQueuer::request).
    fn data(&self, vertices: impl Transfer<Self::Vertex>) -> u32;

    /// Extends the index buffer with the supplied iterator. Indices should be offset by the index
    /// returned by [`data`](VertexQueuer::data).
    fn request(&self, layer: f32, key: <Self::Vertex as Vertex>::PipelineKey, indices: impl Transfer<u32>);
}

/// Marker component for entities that may extract out [`Drawer`]s to the render world. This *must*
/// be added to those entities so they'll be calculated in
/// [`check_visibilities`](crate::vertex::check_visibilities).
#[derive(Reflect, Component, Copy, Clone)]
#[reflect(Component, Default)]
#[require(Visibility)]
pub struct HasDrawer<T: Drawer>(#[reflect(ignore)] pub PhantomData<fn() -> T>);
impl<T: Drawer> Default for HasDrawer<T> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Drawer> HasDrawer<T> {
    /// Shortcut for `HasDrawer(PhantomData)`.
    #[inline]
    pub const fn new() -> Self {
        Self(PhantomData)
    }
}

pub(crate) fn extract_drawers<T: Drawer>(
    mut commands: Commands,
    param: Extract<T::ExtractParam>,
    query: Extract<Query<(RenderEntity, &ViewVisibility, T::ExtractData), (T::ExtractFilter, With<HasDrawer<T>>)>>,
    mut target_query: Query<&mut T>,
) {
    for (e, &view, data) in &query {
        if view.get() {
            if let Ok(mut dst) = target_query.get_mut(e) {
                T::extract(DrawerExtract::Borrowed(&mut dst), &param, data)
            } else {
                let mut extract = None;
                T::extract(DrawerExtract::Spawn(&mut extract), &param, data);

                if let Some(extract) = extract {
                    commands.entity(e).insert(extract);
                }
            }
        }
    }
}

pub(crate) fn queue_drawers<T: Drawer>(
    param: StaticSystemParam<T::DrawParam>,
    buffers: Res<DrawBuffers<T::Vertex>>,
    mut query: Query<(Entity, &mut T, &DrawItems<T::Vertex>)>,
    views: Query<(&RenderVisibleEntities, &VisibleDrawers<T::Vertex>), With<ExtractedView>>,
    mut iterated: Local<FixedBitSet>,
) {
    let buffers = buffers.into_inner();

    iterated.clear();
    for (visible_entities, visible_drawers) in &views {
        let mut iter = query.iter_many_mut(visible_entities.iter::<With<HasDrawer<T>>>().map(|(e, ..)| e));
        while let Some((e, mut drawer, items)) = iter.fetch_next() {
            let index = e.index() as usize;
            if iterated[index] {
                continue;
            }

            iterated.grow_and_insert(index);
            visible_drawers.0.append([e]);

            drawer.draw(&param, &Queuer { buffers, items });
        }
    }

    struct Queuer<'a, T: Vertex> {
        buffers: &'a DrawBuffers<T>,
        items: &'a DrawItems<T>,
    }

    impl<T: Vertex> VertexQueuer for Queuer<'_, T> {
        type Vertex = T;

        #[inline]
        fn data(&self, vertices: impl Transfer<T>) -> u32 {
            self.buffers.vertices.append(vertices) as u32
        }

        #[inline]
        fn request(&self, layer: f32, key: T::PipelineKey, indices: impl Transfer<u32>) {
            let len = indices.len();
            let offset = self.buffers.indices.append(indices);

            self.items
                .0
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push((offset..offset + len, layer, key));
        }
    }
}
