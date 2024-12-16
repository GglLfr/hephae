#![doc = include_str!("../README.md")]
#![cfg_attr(doc, deny(missing_docs))]
#![allow(clippy::type_complexity)]
#![allow(clippy::too_many_arguments)]

pub mod drawer;
pub mod pipeline;
pub mod vertex;

use std::marker::PhantomData;

use bevy_app::prelude::*;
use bevy_asset::prelude::*;
use bevy_core_pipeline::core_2d::Transparent2d;
use bevy_ecs::{prelude::*, system::ReadOnlySystemParam};
use bevy_render::{
    prelude::*,
    render_phase::{AddRenderCommand, RenderCommand},
    render_resource::SpecializedRenderPipelines,
    view::VisibilitySystems,
    Render, RenderApp, RenderSet,
};

use crate::{
    pipeline::{
        clear_batches, extract_shader, load_shader, prepare_batch, prepare_view_bind_groups, queue_vertices, DrawRequests,
        HephaePipeline,
    },
    vertex::{check_visibilities, Vertex, VertexDrawers, VertexQueues},
};

/// Common imports for [`hephae_render`](crate).
pub mod prelude {
    pub use ::bytemuck::{self, NoUninit, Pod, Zeroable};

    pub use crate::{
        drawer::{Drawer, DrawerPlugin, HasDrawer},
        vertex::{Vertex, VertexCommand, VertexQueuer},
        HephaeRenderPlugin, HephaeSystems,
    };
}

/// Global handle to the global shader containing bind groups defining view uniform and tonemapping
/// LUTs.
pub const HEPHAE_VIEW_BINDINGS_HANDLE: Handle<Shader> = Handle::weak_from_u128(278527494526026980866063021704582553601);

/// Labels assigned to Hephae systems that are added to [`Render`].
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, SystemSet)]
pub enum HephaeSystems {
    /// Label for [`clear_batches`], in [`RenderSet::Queue`].
    ClearBatches,
    /// Label for [`queue_drawers`](drawer::queue_drawers), in
    /// [`RenderSet::Queue`].
    QueueDrawers,
    /// Label for [`queue_vertices`], in [`RenderSet::Queue`].
    QueueVertices,
    /// Label for [`prepare_batch`] and [`prepare_view_bind_groups`], in
    /// [`RenderSet::PrepareBindGroups`].
    PrepareBindGroups,
}

/// The entry point of Hephae, generic over `T`.
///
/// Adds the core functionality of Hephae through the
/// [`Vertex`] `impl` of `T`. Note that with this alone you can't start drawing yet; refer to
/// [`DrawerPlugin`](drawer::DrawerPlugin) for more.
pub struct HephaeRenderPlugin<T: Vertex>(PhantomData<fn() -> T>);
impl<T: Vertex> Default for HephaeRenderPlugin<T> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Vertex> HephaeRenderPlugin<T> {
    /// Constructs the plugin for use in [`App::add_plugins`].
    #[inline]
    pub const fn new() -> Self {
        Self(PhantomData)
    }
}

impl<T: Vertex> Plugin for HephaeRenderPlugin<T>
where
    <T::RenderCommand as RenderCommand<Transparent2d>>::Param: ReadOnlySystemParam,
{
    fn build(&self, app: &mut App) {
        struct Common;
        impl Plugin for Common {
            fn build(&self, _: &mut App) {}
        }

        let run = !app.is_plugin_added::<Common>();
        if run {
            app.add_plugins(Common);
            app.world_mut().resource_mut::<Assets<Shader>>().insert(
                &HEPHAE_VIEW_BINDINGS_HANDLE,
                Shader::from_wgsl(include_str!("view_bindings.wgsl"), "hephae/view_bindings.wgsl"),
            );
        }

        app.init_resource::<VertexDrawers<T>>()
            .add_systems(Startup, load_shader::<T>)
            .add_systems(PostUpdate, check_visibilities::<T>.in_set(VisibilitySystems::CheckVisibility));

        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            if run {
                render_app.configure_sets(
                    Render,
                    (
                        (
                            HephaeSystems::ClearBatches,
                            HephaeSystems::QueueDrawers,
                            HephaeSystems::QueueVertices,
                        )
                            .in_set(RenderSet::Queue),
                        HephaeSystems::QueueDrawers.before_ignore_deferred(HephaeSystems::QueueVertices),
                        HephaeSystems::PrepareBindGroups.in_set(RenderSet::PrepareBindGroups),
                    ),
                );
            }

            render_app
                .init_resource::<SpecializedRenderPipelines<HephaePipeline<T>>>()
                .init_resource::<VertexQueues<T>>()
                .add_render_command::<Transparent2d, DrawRequests<T>>()
                .add_systems(ExtractSchedule, extract_shader::<T>)
                .add_systems(
                    Render,
                    (
                        clear_batches::<T>.in_set(HephaeSystems::ClearBatches),
                        queue_vertices::<T>.in_set(HephaeSystems::QueueVertices),
                        (prepare_batch::<T>, prepare_view_bind_groups::<T>).in_set(HephaeSystems::PrepareBindGroups),
                    ),
                );
        }
    }

    fn finish(&self, app: &mut App) {
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app.init_resource::<HephaePipeline<T>>();
        }

        T::setup(app);
    }
}
