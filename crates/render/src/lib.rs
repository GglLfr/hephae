#![allow(internal_features)]
#![cfg_attr(any(docsrs, docsrs_dep), feature(rustdoc_internals, doc_cfg))]
#![doc = include_str!("../README.md")]
#![cfg_attr(doc, deny(missing_docs))]

pub mod attribute;
pub mod drawer;
pub mod image_bind;
pub mod pipeline;
pub mod vertex;

use bevy::{
    app::PluginGroupBuilder,
    asset::weak_handle,
    prelude::*,
    render::{
        Render, RenderApp, RenderSet,
        render_phase::AddRenderCommand,
        render_resource::SpecializedRenderPipelines,
        sync_component::SyncComponentPlugin,
        view::{ExtractedView, VisibilitySystems},
    },
};
use hephae_utils::prelude::*;

use crate::{
    drawer::{DrawBy, Drawer, check_visibilities, extract_drawers, queue_drawers},
    image_bind::{ImageAssetEvents, ImageBindGroups, extract_image_events, validate_image_bind_groups},
    pipeline::{
        DrawBuffers, DrawRequests, VertexPipeline, ViewBatches, ViewIndexBuffer, VisibleDrawers, extract_shader,
        load_shader, prepare_indices, prepare_view_bind_groups, queue_vertices,
    },
    prelude::Vertex,
    vertex::DrawItems,
};

/// Common imports for [`hephae_render`](crate).
pub mod prelude {
    pub use ::bytemuck::{self, NoUninit, Pod, Zeroable};

    pub use crate::{
        HephaeRenderSystems,
        attribute::{
            ByteColorAttrib, ColorAttrib, IsAttribData, LinearRgbaExt as _, Nor, Pos2dAttrib, Pos3dAttrib, Shaper, UvAttrib,
            VertexLayout,
        },
        drawer::{DrawBy, Drawer, DrawerExtract, VertexQueuer},
        image_bind::ImageBindGroups,
        pipeline::{VertexPipeline, ViewBatches},
        vertex::Vertex,
    };
}

pub use bytemuck;
pub use vec_belt;

plugin_conf! {
    /// [`Vertex`]s you can pass to [`RendererPlugin`] to conveniently configure them in one go.
    pub trait VertexConf for Vertex, T => VertexPlugin::<T>::default()
}

plugin_conf! {
    /// [`Drawer`]s you can pass to [`RendererPlugin`] to conveniently configure them in one go.
    pub trait DrawerConf for Drawer, T => DrawerPlugin::<T>::default()
}

plugin_def! {
    /// Vertex renderer driver, generic over `T`.
    ///
    /// Adds the core functionality of Hephae through the
    /// [`Vertex`] `impl` of `T`. Note that with this alone you can't start drawing yet; refer to
    /// [`drawer`] for more.
    pub struct VertexPlugin<T: Vertex>;
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_shader::<T>);

        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            let world = render_app
                .init_resource::<SpecializedRenderPipelines<VertexPipeline<T>>>()
                .init_resource::<ViewBatches<T>>()
                .add_render_command::<T::Item, DrawRequests<T>>()
                .add_systems(ExtractSchedule, extract_shader::<T>)
                .add_systems(Render, queue_vertices::<T>.in_set(HephaeRenderSystems::QueueVertices))
                .world_mut();

            world.register_required_components::<ExtractedView, ViewIndexBuffer<T>>();
            world.register_required_components::<ExtractedView, VisibleDrawers<T>>();
        }
    }

    fn finish(&self, app: &mut App) {
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app
                .init_resource::<DrawBuffers<T>>()
                .init_resource::<VertexPipeline<T>>()
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

plugin_def! {
    /// Vertex specialized drawer driver, generic over `T`.
    ///
    /// Integrates [`Drawer`] into your application for entities to render into the Hephae rendering
    /// pipeline.
    pub struct DrawerPlugin<T: Drawer>;
    fn build(&self, app: &mut App) {
        app.add_plugins(SyncComponentPlugin::<DrawBy<T>>::default())
            .register_type::<DrawBy<T>>()
            .add_systems(PostUpdate, check_visibilities::<T>.in_set(VisibilitySystems::CheckVisibility));

        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app
                .add_systems(ExtractSchedule, extract_drawers::<T>)
                .add_systems(Render, queue_drawers::<T>.in_set(HephaeRenderSystems::QueueDrawers))
                .world_mut()
                .register_required_components::<T, DrawItems<T::Vertex>>();
        }
    }
}

plugin_def! {
    /// The entry point of Hephae. See [`vertex`] and [`drawer`] for more information.
    #[plugin_group]
    pub struct RendererPlugin<V: VertexConf = (), D: DrawerConf = ()>;
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

/// Global handle to the global shader containing bind groups defining view uniform and tonemapping
/// LUTs.
pub const HEPHAE_VIEW_BINDINGS_HANDLE: Handle<Shader> = weak_handle!("c52404ee-d572-46fe-9811-b0209e46309e");

/// Labels assigned to Hephae systems that are added to [`Render`].
#[derive(SystemSet, Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum HephaeRenderSystems {
    /// Label for clearing batches, in [`RenderSet::Queue`].
    ClearBatches,
    /// Label for queueing drawers, in [`RenderSet::Queue`].
    QueueDrawers,
    /// Label for queueing vertices, in [`RenderSet::Queue`].
    QueueVertices,
    /// Label for preparing indices based on sorted render items, in
    /// [`RenderSet::PrepareResources`].
    PrepareIndices,
    /// Label for preparing batches and view bind groups, in [`RenderSet::PrepareBindGroups`].
    PrepareBindGroups,
}
