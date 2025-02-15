#![allow(internal_features)]
#![cfg_attr(any(docsrs, docsrs_dep), feature(rustdoc_internals))]
#![doc = include_str!("../README.md")]
#![cfg_attr(doc, deny(missing_docs))]

pub mod drawer;
pub mod image_bind;
pub mod pipeline;
pub mod vertex;

use bevy_asset::prelude::*;
use bevy_ecs::prelude::*;
use bevy_render::prelude::*;

/// Common imports for [`hephae_render`](crate).
pub mod prelude {
    pub use ::bytemuck::{self, NoUninit, Pod, Zeroable};

    pub use crate::{
        drawer::{Drawer, DrawerExtract, HasDrawer, VertexQueuer},
        image_bind::ImageBindGroups,
        pipeline::{HephaePipeline, ViewBatches},
        vertex::Vertex,
        HephaeRenderSystems,
    };
}

/// App plugins for [`hephae_render`](crate).
pub mod plugin {
    use std::marker::PhantomData;

    use bevy_app::{prelude::*, PluginGroupBuilder};
    use bevy_asset::prelude::*;
    use bevy_ecs::prelude::*;
    use bevy_render::{
        prelude::*,
        render_phase::AddRenderCommand,
        render_resource::SpecializedRenderPipelines,
        sync_component::SyncComponentPlugin,
        view::{ExtractedView, VisibilitySystems},
        Render, RenderApp, RenderSet,
    };
    use hephae_utils::prelude::*;

    use crate::{
        drawer::{extract_drawers, queue_drawers, Drawer, HasDrawer},
        image_bind::{extract_image_events, validate_image_bind_groups, ImageAssetEvents, ImageBindGroups},
        pipeline::{
            extract_shader, load_shader, prepare_indices, prepare_view_bind_groups, queue_vertices, DrawBuffers,
            DrawRequests, HephaePipeline, ViewBatches, ViewIndexBuffer, VisibleDrawers,
        },
        vertex::{check_visibilities, DrawItems, Vertex, VertexDrawers},
        HephaeRenderSystems, HEPHAE_VIEW_BINDINGS_HANDLE,
    };

    plugin_conf! {
        /// [`Vertex`]s you can pass to [`render`] to conveniently configure them in one go.
        pub trait VertexConf for Vertex, T => vertex::<T>()
    }

    plugin_conf! {
        /// [`Drawer`]s you can pass to [`render`] to conveniently configure them in one go.
        pub trait DrawerConf for Drawer, T => drawer::<T>()
    }

    /// The entry point of Hephae. See [`vertex`] and [`drawer`] for more information.
    pub fn render<V: VertexConf, D: DrawerConf>() -> impl PluginGroup {
        struct RenderGroup<V: VertexConf, D: DrawerConf>(PhantomData<(V, D)>);
        impl<V: VertexConf, D: DrawerConf> PluginGroup for RenderGroup<V, D> {
            fn build(self) -> PluginGroupBuilder {
                let mut builder = PluginGroupBuilder::start::<Self>().add(|app: &mut App| {
                    app.world_mut().resource_mut::<Assets<Shader>>().insert(
                        &HEPHAE_VIEW_BINDINGS_HANDLE,
                        Shader::from_wgsl(include_str!("view_bindings.wgsl"), "hephae/view_bindings.wgsl"),
                    );

                    if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
                        render_app
                            .configure_sets(
                                Render,
                                (
                                    (
                                        HephaeRenderSystems::ClearBatches,
                                        HephaeRenderSystems::QueueDrawers,
                                        HephaeRenderSystems::QueueVertices,
                                    )
                                        .in_set(RenderSet::Queue),
                                    HephaeRenderSystems::QueueDrawers.before(HephaeRenderSystems::QueueVertices),
                                    HephaeRenderSystems::PrepareIndices.in_set(RenderSet::PrepareResources),
                                    HephaeRenderSystems::PrepareBindGroups.in_set(RenderSet::PrepareBindGroups),
                                ),
                            )
                            .init_resource::<ImageAssetEvents>()
                            .init_resource::<ImageBindGroups>()
                            .add_systems(ExtractSchedule, extract_image_events)
                            .add_systems(
                                Render,
                                validate_image_bind_groups.before(HephaeRenderSystems::PrepareBindGroups),
                            );
                    }
                });

                builder = V::build(builder);
                D::build(builder)
            }
        }

        RenderGroup::<V, D>(PhantomData)
    }

    /// Vertex renderer driver, generic over `T`.
    ///
    /// Adds the core functionality of Hephae through the
    /// [`Vertex`] `impl` of `T`. Note that with this alone you can't start drawing yet; refer to
    /// [`drawer`] for more.
    pub fn vertex<T: Vertex>() -> impl Plugin {
        struct VertexPlugin<T: Vertex>(PhantomData<T>);
        impl<T: Vertex> Plugin for VertexPlugin<T> {
            fn build(&self, app: &mut App) {
                app.init_resource::<VertexDrawers<T>>()
                    .add_systems(Startup, load_shader::<T>)
                    .add_systems(PostUpdate, check_visibilities::<T>.in_set(VisibilitySystems::CheckVisibility));

                if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
                    let world = render_app
                        .init_resource::<SpecializedRenderPipelines<HephaePipeline<T>>>()
                        .add_render_command::<T::Item, DrawRequests<T>>()
                        .add_systems(ExtractSchedule, extract_shader::<T>)
                        .add_systems(Render, queue_vertices::<T>.in_set(HephaeRenderSystems::QueueVertices))
                        .world_mut();

                    world.register_required_components::<ExtractedView, ViewBatches<T>>();
                    world.register_required_components::<ExtractedView, ViewIndexBuffer<T>>();
                    world.register_required_components::<ExtractedView, VisibleDrawers<T>>();
                }
            }

            fn finish(&self, app: &mut App) {
                if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
                    render_app
                        .init_resource::<DrawBuffers<T>>()
                        .init_resource::<HephaePipeline<T>>()
                        .add_systems(
                            Render,
                            (
                                prepare_indices::<T>.in_set(HephaeRenderSystems::PrepareIndices),
                                prepare_view_bind_groups::<T>.in_set(HephaeRenderSystems::PrepareBindGroups),
                            ),
                        );
                }

                T::setup(app);
            }
        }

        VertexPlugin::<T>(PhantomData)
    }

    /// Vertex specialized drawer driver, generic over `T`.
    ///
    /// Integrates [`Drawer`] into your application for entities to render into the Hephae rendering
    /// pipeline.
    pub fn drawer<T: Drawer>() -> impl Plugin {
        |app: &mut App| {
            app.add_plugins(SyncComponentPlugin::<HasDrawer<T>>::default())
                .register_type::<HasDrawer<T>>()
                .world_mut()
                .resource_scope::<VertexDrawers<T::Vertex>, ()>(|world, mut drawers| drawers.add::<T>(world));

            if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
                render_app
                    .add_systems(ExtractSchedule, extract_drawers::<T>)
                    .add_systems(Render, queue_drawers::<T>.in_set(HephaeRenderSystems::QueueDrawers))
                    .world_mut()
                    .register_required_components::<T, DrawItems<T::Vertex>>();
            }
        }
    }
}

/// Global handle to the global shader containing bind groups defining view uniform and tonemapping
/// LUTs.
pub const HEPHAE_VIEW_BINDINGS_HANDLE: Handle<Shader> = Handle::weak_from_u128(278527494526026980866063021704582553601);

/// Labels assigned to Hephae systems that are added to [`bevy_render::Render`].
#[derive(SystemSet, Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum HephaeRenderSystems {
    /// Label for clearing batches, in [`bevy_render::RenderSet::Queue`].
    ClearBatches,
    /// Label for queueing drawers, in [`bevy_render::RenderSet::Queue`].
    QueueDrawers,
    /// Label for queueing vertices, in [`bevy_render::RenderSet::Queue`].
    QueueVertices,
    /// Label for preparing indices based on sorted render items, in
    /// [`bevy_render::RenderSet::PrepareResources`].
    PrepareIndices,
    /// Label for prepating batches and view bind groups, in
    /// [`bevy_render::RenderSet::PrepareBindGroups`].
    PrepareBindGroups,
}
