//! Defines base drawers that work with vertices and supply various vertex commands.

use std::{any::TypeId, marker::PhantomData, sync::PoisonError};

use bevy::{
    ecs::{
        component::Mutable,
        entity::EntityHashMap,
        query::{QueryFilter, QueryItem, ReadOnlyQueryData},
        system::{ReadOnlySystemParam, StaticSystemParam, SystemBuffer, SystemMeta, SystemParamItem, lifetimeless::Write},
    },
    platform::collections::hash_map::Entry,
    prelude::*,
    render::{
        Extract,
        primitives::{Aabb, Frustum},
        sync_world::RenderEntity,
        view::{
            ExtractedView, NoCpuCulling, NoFrustumCulling, RenderLayers, RenderVisibleEntities, VisibilityRange,
            VisibleEntities, VisibleEntityRanges,
        },
    },
    utils::Parallel,
};
use fixedbitset::FixedBitSet;
use vec_belt::Transfer;

use crate::{
    pipeline::{DrawBuffers, VisibleDrawers},
    vertex::{DrawItems, Vertex},
};

/// A render world [`Component`] extracted from the main world that will be used to issue draw
/// requests.
pub trait Drawer: TypePath + Component<Mutability = Mutable> + Sized {
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
    where T: Default {
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

#[derive(FromWorld)]
pub(crate) struct UpdateVisibilities<T: Drawer> {
    entities: EntityHashMap<Vec<Entity>>,
    query: QueryState<Write<VisibleEntities>>,
    _marker: PhantomData<fn() -> T>,
}

impl<T: Drawer> SystemBuffer for UpdateVisibilities<T> {
    fn apply(&mut self, _: &SystemMeta, world: &mut World) {
        //  There is no `iter_manual_mut`, unfortunately.
        let mut query = unsafe { self.query.query_unchecked(world.as_unsafe_world_cell()) };
        for (&view, entities) in &mut self.entities {
            let Ok(mut visible) = query.get_mut(view) else {
                entities.clear();
                continue
            };

            match visible.entities.entry(TypeId::of::<With<T>>()) {
                Entry::Occupied(e) => {
                    let vec = e.into_mut();
                    vec.clear();
                    vec.append(entities);
                }
                Entry::Vacant(e) => {
                    let mut vec = Vec::with_capacity(entities.len());
                    vec.append(entities);
                    e.insert(vec);
                }
            }
        }
    }
}

pub(crate) fn check_visibilities<T: Drawer>(
    view_query: Query<(Entity, &Frustum, Option<&RenderLayers>, &Camera, Has<NoCpuCulling>)>,
    mut visible_aabb_query: Query<
        (
            Entity,
            &InheritedVisibility,
            &mut ViewVisibility,
            Option<&RenderLayers>,
            Option<&Aabb>,
            Option<&GlobalTransform>,
            Has<NoFrustumCulling>,
            Has<VisibilityRange>,
        ),
        (T::ExtractFilter, With<HasDrawer<T>>),
    >,
    visible_entity_ranges: Option<Res<VisibleEntityRanges>>,
    mut update_visibilities: Deferred<UpdateVisibilities<T>>,
    mut thread_queues: Local<Parallel<Vec<Entity>>>,
) {
    for (view_entity, &frustum, maybe_view_mask, camera, no_cpu_culling) in &view_query {
        if !camera.is_active {
            continue
        }

        let view_mask = maybe_view_mask.unwrap_or_default();
        visible_aabb_query.par_iter_mut().for_each_init(
            || thread_queues.borrow_local_mut(),
            |queue,
             (
                entity,
                inherited_visibility,
                mut view_visibility,
                maybe_entity_mask,
                maybe_model_aabb,
                maybe_transform,
                no_frustum_culling,
                has_visibility_range,
            )| {
                if !inherited_visibility.get() {
                    return;
                }

                let entity_mask = maybe_entity_mask.unwrap_or_default();
                if !view_mask.intersects(entity_mask) {
                    return;
                }

                // If outside of the visibility range, cull.
                if has_visibility_range &&
                    visible_entity_ranges.as_deref().is_some_and(|visible_entity_ranges| {
                        !visible_entity_ranges.entity_is_in_range_of_view(entity, view_entity)
                    })
                {
                    return;
                }

                // If there is no transform, just draw it anyway.
                let Some(transform) = maybe_transform else {
                    view_visibility.set();
                    queue.push(entity);

                    return
                };

                // If we have an AABB, do frustum culling.
                if !no_frustum_culling && !no_cpu_culling {
                    if let Some(model_aabb) = maybe_model_aabb {
                        let world_from_local = transform.affine();
                        let model_sphere = bevy::render::primitives::Sphere {
                            center: world_from_local.transform_point3a(model_aabb.center),
                            radius: transform.radius_vec3a(model_aabb.half_extents),
                        };

                        // Do quick sphere-based frustum culling.
                        if !frustum.intersects_sphere(&model_sphere, false) {
                            return
                        }

                        // Do AABB-based frustum culling.
                        if !frustum.intersects_obb(model_aabb, &world_from_local, true, false) {
                            return
                        }
                    }
                }

                view_visibility.set();
                queue.push(entity)
            },
        );

        thread_queues.drain_into(match update_visibilities.entities.entry(view_entity) {
            Entry::Occupied(e) => {
                let vec = e.into_mut();
                vec.clear();
                vec
            }
            Entry::Vacant(e) => e.insert(Vec::new()),
        })
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
        let mut iter = query.iter_many_mut(visible_entities.iter::<With<T>>().map(|(e, ..)| e));
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
