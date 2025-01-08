//! Defines base drawers that work with vertices and supply various vertex commands.

use std::{marker::PhantomData, sync::PoisonError};

use bevy_app::prelude::*;
use bevy_ecs::{
    prelude::*,
    query::{QueryFilter, QueryItem, ReadOnlyQueryData},
    system::{ReadOnlySystemParam, StaticSystemParam, SystemParamItem},
};
use bevy_reflect::prelude::*;
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
    HephaeRenderSystems,
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

impl<T: Drawer> Plugin for DrawerPlugin<T> {
    fn build(&self, app: &mut App) {
        app.add_plugins(SyncComponentPlugin::<HasDrawer<T>>::default())
            .register_type::<HasDrawer<T>>()
            .world_mut()
            .resource_scope::<VertexDrawers<T::Vertex>, ()>(|world, mut drawers| drawers.add::<T>(world));

        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app
                .add_systems(ExtractSchedule, extract_drawers::<T>)
                .add_systems(Render, queue_drawers::<T>.in_set(HephaeRenderSystems::QueueDrawers));
        }
    }
}

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

    /// Issues [`VertexCommand`](crate::vertex::VertexCommand)s for rendering, in a form of Z-layer,
    /// [pipeline key](Vertex::PipelineKey), and [vertex command](Vertex::Command).
    fn enqueue(
        &self,
        param: &SystemParamItem<Self::DrawParam>,
        queuer: &mut impl Extend<(f32, <Self::Vertex as Vertex>::PipelineKey, <Self::Vertex as Vertex>::Command)>,
    );
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
        self.get_mut(Default::default)
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

/// Extracts an instance of `T` from matching entities.
pub fn extract_drawers<T: Drawer>(
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
