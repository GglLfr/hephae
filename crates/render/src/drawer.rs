//! Defines base drawers that work with vertices and supply various vertex commands.

use std::{marker::PhantomData, sync::PoisonError};

use bevy_app::prelude::*;
use bevy_ecs::{
    prelude::*,
    query::{QueryFilter, QueryItem, ReadOnlyQueryData},
    system::{ReadOnlySystemParam, StaticSystemParam, SystemParamItem},
};
use bevy_render::{
    prelude::*,
    sync_component::SyncComponentPlugin,
    sync_world::RenderEntity,
    view::{ExtractedView, RenderVisibleEntities},
    Extract, Render, RenderApp,
};
use fixedbitset::FixedBitSet;

use crate::{
    vertex::{Vertex, VertexDrawers, VertexQueues},
    HephaeSystems,
};

/// Integrates [`Drawer`] into your application for entities to render into the Hephae rendering
/// pipeline.
pub struct DrawerPlugin<D: Drawer>(PhantomData<fn() -> D>);
impl<T: Drawer> Default for DrawerPlugin<T> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<D: Drawer> DrawerPlugin<D> {
    /// Shortcut for `DrawerPlugin(PhantomData)`.
    pub const fn new() -> Self {
        Self(PhantomData)
    }
}

impl<D: Drawer> Plugin for DrawerPlugin<D> {
    fn build(&self, app: &mut App) {
        app.add_plugins(SyncComponentPlugin::<HasDrawer<D>>::default());
        app.world_mut()
            .resource_scope::<VertexDrawers<D::Vertex>, ()>(|world, mut drawers| drawers.add::<D>(world));

        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app
                .add_systems(ExtractSchedule, extract_drawers::<D>)
                .add_systems(Render, queue_drawers::<D>.in_set(HephaeSystems::QueueDrawers));
        }
    }
}

/// A render world [`Component`] extracted from the main world that will be used to issue
/// [`VertexCommand`](crate::vertex::VertexCommand)s.
pub trait Drawer: Component + Sized {
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
    fn extract(param: &SystemParamItem<Self::ExtractParam>, query: QueryItem<Self::ExtractData>) -> Option<Self>;

    /// Issues [`VertexCommand`](crate::vertex::VertexCommand)s for rendering, in a form of Z-layer,
    /// [pipeline key](Vertex::PipelineKey), and [vertex command](Vertex::Command).
    fn enqueue(
        &self,
        param: &SystemParamItem<Self::DrawParam>,
        queuer: &mut impl Extend<(f32, <Self::Vertex as Vertex>::PipelineKey, <Self::Vertex as Vertex>::Command)>,
    );
}

/// Marker component for entities that may extract out [`Drawer`]s to the render world. This *must*
/// be added to those entities so they'll be calculated in
/// [`check_visibilities`](crate::vertex::check_visibilities).
#[derive(Component, Copy, Clone)]
pub struct HasDrawer<T: Drawer>(pub PhantomData<fn() -> T>);
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

/// Extracts an instance of `T` from matching entities.
pub fn extract_drawers<T: Drawer>(
    mut commands: Commands,
    param: Extract<T::ExtractParam>,
    query: Extract<Query<(RenderEntity, &ViewVisibility, T::ExtractData), (T::ExtractFilter, With<HasDrawer<T>>)>>,
) {
    for (e, &view, data) in &query {
        if view.get() {
            if let Some(out) = T::extract(&param, data) {
                commands.entity(e).insert(out);
            } else {
                commands.entity(e).remove::<T>();
            }
        }
    }
}

/// Collects [`VertexCommand`](crate::vertex::VertexCommand)s from drawers to be sorted by the
/// pipeline.
pub fn queue_drawers<T: Drawer>(
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
}
