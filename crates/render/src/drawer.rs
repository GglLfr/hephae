//! Defines base drawers that work with vertices and supply various vertex commands.

use std::marker::PhantomData;

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

use crate::vertex::Vertex;

/// A render world [`Component`] extracted from the main world that will be used to issue
/// [`VertexCommand`](crate::vertex::VertexCommand)s.
pub trait Drawer: TypePath + Component + Sized {
    /// The type of vertex this drawer works with.
    type Vertex: Vertex;

    /// System parameter to fetch when extracting data from the main world.
    type ExtractParam: ReadOnlySystemParam;
    /// Query item to fetch from entities when extracting from those entities to the render world.
    type ExtractData: ReadOnlyQueryData;
    /// Additional query filters accompanying [`ExtractData`](Drawer::ExtractData).
    type ExtractFilter: QueryFilter;

    /// System parameter to fetch when issuing [`VertexCommand`](crate::vertex::VertexCommand)s.
    type DrawParam: ReadOnlySystemParam;

    /// Extracts an instance of this drawer from matching entities, if available.
    fn extract(
        drawer: DrawerExtract<Self>,
        param: &SystemParamItem<Self::ExtractParam>,
        query: QueryItem<Self::ExtractData>,
    );

    /// Issues vertex data and draw requests for the data.
    fn draw(&self, param: &SystemParamItem<Self::DrawParam>, queuer: &impl VertexQueuer<Vertex = Self::Vertex>);
}

/// Similar to [`Extend`], except it works with both vertex and index buffers.
///
/// Ideally it also adjusts the index offset to the length of the current vertex buffer so
/// primitives would have the correct shapes.
pub trait VertexQueuer {
    /// The type of vertex this queuer works with.
    type Vertex: Vertex;

    /// Extends the vertex buffer with the supplied iterator. Returns the base index that should be
    /// used for [`request`](VertexQueuer::request).
    fn data(&self, vertices: impl AsRef<[Self::Vertex]>) -> usize;

    /// Extends the index buffer with the supplied iterator.
    fn request(&self, prop: <Self::Vertex as Vertex>::PipelineProp, base_index: usize, indices: impl AsRef<[u32]>);
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
    query: Query<&T>,
    views: Query<(Entity, &RenderVisibleEntities), With<ExtractedView>>,
) {
    for (view_entity, visible_entities) in &views {
        for &(e, main_e) in visible_entities.iter::<With<HasDrawer<T>>>() {
            let Ok(drawer) = query.get(e) else { continue };

            drawer.draw(&param, queuer);
        }
    }
}

/*pub(crate) fn queue_drawers<T: Drawer>(
    param: StaticSystemParam<T::DrawParam>,
    query: Query<&T>,
    views: Query<(Entity, &RenderVisibleEntities), With<ExtractedView>>,
    queues: Res<VertexQueues<T::Vertex>>,
    mut iterated: Local<FixedBitSet>,
) {
    iterated.clear();
    for (view_entity, visible_entities) in &views {
        for &(e, main_e) in visible_entities.iter::<With<HasDrawer<T>>>() {
            let index = e.index() as usize;
            if iterated[index] {
                continue;
            }

            let Ok(drawer) = query.get(e) else { continue };

            iterated.grow_and_insert(index);
            queues.entities.entry(view_entity).or_default().insert((e, main_e));

            drawer.enqueue(&param, &mut *queues.commands.entry(e).or_default());
        }
    }

    queues
        .entity_bits
        .write()
        .unwrap_or_else(PoisonError::into_inner)
        .union_with(&iterated);
}*/
