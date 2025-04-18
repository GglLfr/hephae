//! The heart of Hephae.
//!
//! See the documentation of [Vertex] for more information.

use std::{hash::Hash, ops::Range};

use bevy::{
    core_pipeline::core_2d::{CORE_2D_DEPTH_FORMAT, Transparent2d},
    ecs::system::{ReadOnlySystemParam, SystemParam, SystemParamItem},
    math::FloatOrd,
    platform::sync::Mutex,
    prelude::*,
    render::{
        render_phase::{CachedRenderPipelinePhaseItem, DrawFunctionId, PhaseItemExtraIndex, RenderCommand, SortedPhaseItem},
        render_resource::{CachedRenderPipelineId, RenderPipelineDescriptor, TextureFormat},
        sync_world::MainEntity,
    },
};
use smallvec::SmallVec;

use crate::attribute::VertexLayout;

/// A [`PhaseItem`](bevy::render::render_phase::PhaseItem) that works with [`Vertex`].
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

/// Implements [`DrawerPhaseItem`] for [`Transparent2d`] with its `extracted_index` field.
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
    /// [`BindGroupLayout`](bevy::render::render_resource::BindGroupLayout) for texture-sampling.
    type PipelineProp: Send + Sync;
    /// Key used to specialize the render pipeline. For example, this may be an
    /// [`AssetId<Image>`](Handle<bevy::image::Image>) used to reference a
    /// [`GpuImage`](bevy::render::texture::GpuImage) for texture-sampling.
    type PipelineKey: Send + Sync + Clone + Eq + PartialEq + Hash;
    /// Format of the depth-stencil pass supplied to the rendering pipeline creation parameters.
    /// Defaults to [`Some(TextureFormat::Depth32Float)`], which is the default for 2D core pipeline
    /// depth-stencil format. [`None`] means the pipeline will not have a depth-stencil state.
    const DEPTH_FORMAT: Option<TextureFormat> = Some(CORE_2D_DEPTH_FORMAT);

    /// System parameter to fetch when [creating the batch](Vertex::create_batch).
    type BatchParam: SystemParam;
    /// Additional property that is embedded into the [batch](crate::pipeline::ViewBatches)
    /// components for use in [`RenderCommand`](Vertex::RenderCommand). For example, this may be
    /// an [`AssetId<Image>`](Handle<bevy::image::Image>) from
    /// [`PipelineKey`](Vertex::PipelineKey) to attach the associated bind
    /// group for texture-sampling.
    type BatchProp: Send + Sync;

    /// The [`PhaseItem`](bevy::render::render_phase::PhaseItem) that this vertex works with.
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

#[derive(Component)]
pub(crate) struct DrawItems<T: Vertex>(pub Mutex<SmallVec<[(Range<usize>, f32, T::PipelineKey); 8]>>);
impl<T: Vertex> Default for DrawItems<T> {
    #[inline]
    fn default() -> Self {
        Self(Mutex::new(SmallVec::new()))
    }
}
