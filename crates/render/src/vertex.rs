//! The heart of Hephae.
//!
//! See the documentation of [Vertex] for more information.

use std::{any::TypeId, hash::Hash, marker::PhantomData, ops::Range, sync::Mutex};

use bevy::{
    core_pipeline::core_2d::{CORE_2D_DEPTH_FORMAT, Transparent2d},
    ecs::{
        component::ComponentId,
        entity::EntityHashMap,
        storage::SparseSet,
        system::{ReadOnlySystemParam, SystemParam, SystemParamItem, SystemState},
        world::FilteredEntityRef,
    },
    math::FloatOrd,
    prelude::*,
    render::{
        primitives::{Aabb, Frustum},
        render_phase::{CachedRenderPipelinePhaseItem, DrawFunctionId, PhaseItemExtraIndex, RenderCommand, SortedPhaseItem},
        render_resource::{CachedRenderPipelineId, RenderPipelineDescriptor, TextureFormat},
        sync_world::MainEntity,
        view::{NoCpuCulling, NoFrustumCulling, RenderLayers, VisibilityRange, VisibleEntities, VisibleEntityRanges},
    },
    utils::{Parallel, TypeIdMap},
};
use smallvec::SmallVec;

use crate::{
    attribute::VertexLayout,
    drawer::{Drawer, HasDrawer},
};

/// A [`PhaseItem`](bevy_render::render_phase::PhaseItem) that works with [`Vertex`].
///
/// The phase item is special in that it's aware of which draw request from a [`Drawer`] it's
/// actually rendering. This means, multiple [`DrawerPhaseItem`]s may point to the same entities but
/// draw different things.
pub trait DrawerPhaseItem: CachedRenderPipelinePhaseItem + SortedPhaseItem {
    /// Creates the phase item associated with a [`Drawer`] based on its layer, render and main
    /// entity, rendering pipeline ID, draw function ID, and command index.
    fn create(
        layer: f32,
        entity: (Entity, MainEntity),
        pipeline: CachedRenderPipelineId,
        draw_function: DrawFunctionId,
        command: usize,
    ) -> Self;

    /// Returns the associated draw request index.
    fn command(&self) -> usize;
}

impl DrawerPhaseItem for Transparent2d {
    #[inline]
    fn create(
        layer: f32,
        entity: (Entity, MainEntity),
        pipeline: CachedRenderPipelineId,
        draw_function: DrawFunctionId,
        command: usize,
    ) -> Self {
        Self {
            sort_key: FloatOrd(layer),
            entity,
            pipeline,
            draw_function,
            batch_range: 0..0,
            extracted_index: command,
            extra_index: PhaseItemExtraIndex::None,
            indexed: true,
        }
    }

    #[inline]
    fn command(&self) -> usize {
        self.extracted_index
    }
}

/// The heart of Hephae. Instances of `Vertex` directly represent the elements of the vertex buffer
/// in the GPU.
pub trait Vertex: Send + Sync + VertexLayout {
    /// System parameter to fetch when initializing
    /// [`VertexPipeline`](crate::pipeline::VertexPipeline) to create a
    /// [`PipelineProp`](Vertex::PipelineProp).
    type PipelineParam: SystemParam;
    /// The additional property of the [common pipeline definition](crate::pipeline::VertexPipeline)
    /// that may used when specializing based on [`PipelineKey`](Vertex::PipelineKey). For example,
    /// this may be used to create a
    /// [`BindGroupLayout`](bevy_render::render_resource::BindGroupLayout) for texture-sampling.
    type PipelineProp: Send + Sync;
    /// Key used to specialize the render pipeline. For example, this may be an
    /// [`AssetId<Image>`](bevy_asset::Handle<bevy_image::Image>) used to reference a
    /// [`GpuImage`](bevy_render::texture::GpuImage) for texture-sampling.
    type PipelineKey: Send + Sync + Clone + Eq + PartialEq + Hash;
    /// Format of the depth-stencil pass supplied to the rendering pipeline creation parameters.
    /// Defaults to [`Some(TextureFormat::Depth32Float)`], which is the default for 2D core pipeline
    /// depth-stencil format. [`None`] means the pipeline will not have a depth-stencil state.
    const DEPTH_FORMAT: Option<TextureFormat> = Some(CORE_2D_DEPTH_FORMAT);

    /// System parameter to fetch when [creating the batch](Vertex::create_batch).
    type BatchParam: SystemParam;
    /// Additional property that is embedded into the [batch](crate::pipeline::ViewBatches)
    /// components for use in [`RenderCommand`](Vertex::RenderCommand). For example, this may be
    /// an [`AssetId<Image>`](bevy_asset::Handle<bevy_image::Image>) from
    /// [`PipelineKey`](Vertex::PipelineKey) to attach the associated bind
    /// group for texture-sampling.
    type BatchProp: Send + Sync;

    /// The [`PhaseItem`](bevy_render::render_phase::PhaseItem) that this vertex works with.
    type Item: DrawerPhaseItem;
    /// Additional GPU render commands to invoke before actually drawing the vertex and index
    /// buffers. For example, this may be used to set the texture-sampling bind group provided by
    /// [`BatchProp`](Vertex::BatchProp).
    type RenderCommand: RenderCommand<Self::Item, Param: ReadOnlySystemParam> + Send + Sync;

    /// Path to the shader rendering vertex attributes of this type. Entry points should be
    /// `vertex(...)` and `fragment(...)`.
    const SHADER: &'static str;

    /// Further customizes the application. Called in [`Plugin::finish`]. For example, this may be
    /// used to add systems extracting texture atlas pages and validating bind groups associated
    /// with them.
    fn setup(#[allow(unused)] app: &mut App) {}

    /// Creates the additional render pipeline property for use in
    /// [specialization](Vertex::specialize_pipeline).
    fn init_pipeline(param: SystemParamItem<Self::PipelineParam>) -> Self::PipelineProp;

    /// Specializes the render pipeline descriptor based off of the [key](Vertex::PipelineKey) and
    /// [prop](Vertex::PipelineProp) of the common render pipeline descriptor.
    #[allow(unused)]
    fn specialize_pipeline(key: Self::PipelineKey, prop: &Self::PipelineProp, desc: &mut RenderPipelineDescriptor) {}

    /// Creates additional batch property for use in rendering.
    fn create_batch(param: &mut SystemParamItem<Self::BatchParam>, key: Self::PipelineKey) -> Self::BatchProp;
}

/// Stores the runtime-only type information of [`Drawer`] that is associated with a [`Vertex`] for
/// use in [`check_visibilities`].
#[derive(Resource)]
pub struct VertexDrawers<T: Vertex>(pub(crate) SparseSet<ComponentId, TypeId>, PhantomData<fn() -> T>);
impl<T: Vertex> Default for VertexDrawers<T> {
    #[inline]
    fn default() -> Self {
        Self(SparseSet::new(), PhantomData)
    }
}

impl<T: Vertex> VertexDrawers<T> {
    /// Registers a [`Drawer`] to be checked in [`check_visibilities`].
    #[inline]
    pub fn add<D: Drawer<Vertex = T>>(&mut self, world: &mut World) {
        self.0
            .insert(world.register_component::<HasDrawer<D>>(), TypeId::of::<With<HasDrawer<D>>>());
    }
}

#[derive(Component)]
pub(crate) struct DrawItems<T: Vertex>(pub Mutex<SmallVec<[(Range<usize>, f32, T::PipelineKey); 8]>>);
impl<T: Vertex> Default for DrawItems<T> {
    #[inline]
    fn default() -> Self {
        Self(Mutex::new(SmallVec::new()))
    }
}

/// Calculates [`ViewVisibility`] of [drawable](Drawer) entities.
///
/// Similar to [`check_visibility`](bevy_render::view::check_visibility) that is generic over
/// [`HasDrawer`], except the filters are configured dynamically by
/// [`DrawerPlugin`](crate::DrawerPlugin). This makes it so that all drawers that share the
/// same [`Vertex`] type also share the same visibility system.
pub fn check_visibilities<T: Vertex>(
    world: &mut World,
    visibility: &mut QueryState<FilteredEntityRef>,
    views: &mut SystemState<(
        Query<(Entity, &Frustum, Option<&RenderLayers>, &Camera, Has<NoCpuCulling>)>,
        Query<(
            Entity,
            &InheritedVisibility,
            &mut ViewVisibility,
            Option<&RenderLayers>,
            Option<&Aabb>,
            &GlobalTransform,
            Has<NoFrustumCulling>,
            Has<VisibilityRange>,
        )>,
        Option<Res<VisibleEntityRanges>>,
    )>,
    visible_entities: &mut SystemState<Query<(Entity, &mut VisibleEntities)>>,
    mut thread_queues: Local<Parallel<Vec<Entity>>>,
    mut view_queues: Local<EntityHashMap<Vec<Entity>>>,
    mut view_maps: Local<EntityHashMap<TypeIdMap<Vec<Entity>>>>,
) {
    world.resource_scope(|world, drawers: Mut<VertexDrawers<T>>| {
        if drawers.is_changed() {
            let mut builder = QueryBuilder::<FilteredEntityRef>::new(world);
            builder.or(|query| {
                for id in drawers.0.indices() {
                    query.with_id(id);
                }
            });

            *visibility = builder.build();
        }
    });

    let (view_query, mut visible_aabb_query, visible_entity_ranges) = views.get_mut(world);
    let visible_entity_ranges = visible_entity_ranges.as_deref();
    for (view, &frustum, maybe_view_mask, camera, no_cpu_culling) in &view_query {
        if !camera.is_active {
            continue;
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
                transform,
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
                    visible_entity_ranges.is_some_and(|visible_entity_ranges| {
                        !visible_entity_ranges.entity_is_in_range_of_view(entity, view)
                    })
                {
                    return;
                }

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
                            return;
                        }

                        // Do AABB-based frustum culling.
                        if !frustum.intersects_obb(model_aabb, &world_from_local, true, false) {
                            return;
                        }
                    }
                }

                view_visibility.set();
                queue.push(entity);
            },
        );

        thread_queues.drain_into(view_queues.entry(view).or_default());
    }

    visibility.update_archetypes(world);

    let drawers = world.resource::<VertexDrawers<T>>();
    for (&view, queues) in &mut view_queues {
        let map = view_maps.entry(view).or_default();
        for e in queues.drain(..) {
            let Ok(visible) = visibility.get_manual(world, e) else {
                continue;
            };

            for (&id, &key) in drawers.0.iter() {
                if visible.contains_id(id) {
                    map.entry(key).or_default().push(e);
                }
            }
        }
    }

    let mut visible_entities = visible_entities.get_mut(world);
    for (view, mut visible_entities) in &mut visible_entities {
        let Some(map) = view_maps.get_mut(&view) else {
            continue;
        };
        for (&id, entities) in map {
            let dst = visible_entities.entities.entry(id).or_default();
            dst.clear();
            dst.append(entities);
        }
    }
}
