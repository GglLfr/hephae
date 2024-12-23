//! Provides all the necessary resources for a working base rendering pipeline. Note that
//! drawer-specific pipeline integration is provided by
//! [`DrawerPlugin`](crate::drawer::DrawerPlugin).
//!
//! The procedures are as following:
//! - During [extraction](ExtractSchedule), the [pipeline shader](Vertex::SHADER) [id](AssetId) is
//!   synchronized from the main world to the render world.
//! - During [phase item queueing](bevy_render::RenderSet::Queue), vertex and index buffers for use
//!   in batches are inserted (or cleared if exists already) to each [view](ExtractedView) entities.
//!   Each visible [drawers](crate::drawer::Drawer) also queue [`VertexCommand`]s as [phase
//!   items](PhaseItem), ready to be sorted.
//! - During [GPU resource preparation](bevy_render::RenderSet::PrepareBindGroups), camera view bind
//!   groups are created, and for each camera view, overlapping vertex commands are invoked to draw
//!   into the GPU buffers. Compatible vertex commands are batched, that is, they share a section in
//!   the vertex and index buffers and share GPU render calls.
//! - [`DrawRequests`] renders each batch.

use std::{marker::PhantomData, ops::Range, sync::PoisonError};

use bevy_asset::prelude::*;
use bevy_core_pipeline::tonemapping::{
    get_lut_bind_group_layout_entries, get_lut_bindings, DebandDither, Tonemapping, TonemappingLuts,
};
use bevy_ecs::{
    prelude::*,
    query::ROQueryItem,
    system::{lifetimeless::Read, ReadOnlySystemParam, SystemParamItem, SystemState},
};
use bevy_image::BevyDefault;
use bevy_render::{
    prelude::*,
    render_asset::RenderAssets,
    render_phase::{
        DrawFunctions, PhaseItem, PhaseItemExtraIndex, RenderCommand, RenderCommandResult, SetItemPipeline,
        TrackedRenderPass, ViewSortedRenderPhases,
    },
    render_resource::{
        binding_types::uniform_buffer, BindGroup, BindGroupEntry, BindGroupLayout, BindingResource, BlendState,
        BufferAddress, BufferUsages, ColorTargetState, ColorWrites, CompareFunction, DepthBiasState, DepthStencilState,
        FragmentState, FrontFace, IndexFormat, MultisampleState, PipelineCache, PolygonMode, PrimitiveState,
        PrimitiveTopology, RawBufferVec, RenderPipelineDescriptor, ShaderDefVal, ShaderStages, SpecializedRenderPipeline,
        SpecializedRenderPipelines, StencilFaceState, StencilState, TextureFormat, VertexBufferLayout, VertexState,
        VertexStepMode,
    },
    renderer::{RenderDevice, RenderQueue},
    texture::{FallbackImage, GpuImage},
    view::{ExtractedView, ViewTarget, ViewUniform, ViewUniformOffset, ViewUniforms},
    Extract,
};

use crate::vertex::{Vertex, VertexCommand, VertexQueuer, VertexQueues};

/// Common pipeline descriptor for use in [specialization](Vertex::specialize_pipeline). See the
/// module-level documentation.
#[derive(Resource)]
pub struct HephaePipeline<T: Vertex> {
    view_layout: BindGroupLayout,
    vertex_prop: T::PipelineProp,
}

impl<T: Vertex> HephaePipeline<T> {
    /// Returns the [additional property](Vertex::PipelineProp) of the vertex definition for use in
    /// [specialization](Vertex::specialize_pipeline).
    #[inline]
    pub const fn vertex_prop(&self) -> &T::PipelineProp {
        &self.vertex_prop
    }
}

impl<T: Vertex> FromWorld for HephaePipeline<T> {
    fn from_world(world: &mut World) -> Self {
        let device = world.resource::<RenderDevice>();

        let [lut_texture, lut_sampler] = get_lut_bind_group_layout_entries();
        let view_layout = device.create_bind_group_layout("hephae_view_layout", &[
            uniform_buffer::<ViewUniform>(true).build(0, ShaderStages::VERTEX_FRAGMENT),
            lut_texture.build(1, ShaderStages::FRAGMENT),
            lut_sampler.build(2, ShaderStages::FRAGMENT),
        ]);

        let mut state = SystemState::<T::PipelineParam>::new(world);
        let vertex_prop = T::init_pipeline(state.get_mut(world));
        state.apply(world);

        Self {
            view_layout,
            vertex_prop,
        }
    }
}

/// Asset handle to the [pipeline shader](Vertex::SHADER).
#[derive(Resource)]
pub struct PipelineShader<T: Vertex>(pub(crate) Handle<Shader>, PhantomData<fn() -> T>);
impl<T: Vertex> PipelineShader<T> {
    /// Returns the [`AssetId<Shader>`] to the [pipeline shader](Vertex::SHADER).
    #[inline]
    pub fn shader(&self) -> AssetId<Shader> {
        self.0.id()
    }
}

/// [`Startup`](bevy_app::Startup) system that loads the [`PipelineShader`].
pub fn load_shader<T: Vertex>(mut commands: Commands, server: Res<AssetServer>) {
    commands.insert_resource(PipelineShader::<T>(server.load(T::SHADER), PhantomData));
}

/// Extracts the [`PipelineShader`] resource from the main world to the render world for use in
/// pipeline specialization.
pub fn extract_shader<T: Vertex>(mut commands: Commands, shader: Extract<Option<Res<PipelineShader<T>>>>) {
    if let Some(ref shader) = *shader {
        if shader.is_changed() {
            commands.insert_resource(PipelineShader::<T>(shader.0.clone_weak(), PhantomData));
        }
    }
}

/// Common pipeline specialization key.
///
/// Factors components from [views](ExtractedView) such as [HDR](ExtractedView::hdr),
/// [multisampling](Msaa), [tonemapping](Tonemapping), and [deband-dithering](DebandDither).
#[derive(Eq, PartialEq, Hash, Copy, Clone)]
pub struct ViewKey {
    /// Whether HDR is turned on.
    pub hdr: bool,
    /// MSAA samples, represented as its trailing zeroes.
    pub msaa: u8,
    /// Whether tonemapping is enabled, and what method is used.
    pub tonemapping: Option<Tonemapping>,
    /// Whether deband-dithering is enabled.
    pub dither: bool,
    /// The asset ID of the [shader](Vertex::SHADER). May be turned into a [`Handle`] by using
    /// [`Handle::Weak`].
    pub shader: AssetId<Shader>,
}

impl<T: Vertex> SpecializedRenderPipeline for HephaePipeline<T> {
    type Key = (ViewKey, T::PipelineKey);

    fn specialize(&self, key: Self::Key) -> RenderPipelineDescriptor {
        let (view_key, key) = key;
        let mut defs = Vec::new();
        if let Some(tonemapping) = view_key.tonemapping {
            defs.extend([
                "TONEMAP_IN_SHADER".into(),
                ShaderDefVal::UInt("TONEMAPPING_LUT_TEXTURE_BINDING_INDEX".into(), 1),
                ShaderDefVal::UInt("TONEMAPPING_LUT_SAMPLER_BINDING_INDEX".into(), 2),
                match tonemapping {
                    Tonemapping::None => "TONEMAP_METHOD_NONE",
                    Tonemapping::Reinhard => "TONEMAP_METHOD_REINHARD",
                    Tonemapping::ReinhardLuminance => "TONEMAP_METHOD_REINHARD_LUMINANCE",
                    Tonemapping::AcesFitted => "TONEMAP_METHOD_ACES_FITTED",
                    Tonemapping::AgX => "TONEMAP_METHOD_AGX",
                    Tonemapping::SomewhatBoringDisplayTransform => "TONEMAP_METHOD_SOMEWHAT_BORING_DISPLAY_TRANSFORM",
                    Tonemapping::TonyMcMapface => "TONEMAP_METHOD_TONY_MC_MAPFACE",
                    Tonemapping::BlenderFilmic => "TONEMAP_METHOD_BLENDER_FILMIC",
                }
                .into(),
            ]);

            if view_key.dither {
                defs.push("DEBAND_DITHER".into());
            }
        }

        let format = match view_key.hdr {
            true => ViewTarget::TEXTURE_FORMAT_HDR,
            false => TextureFormat::bevy_default(),
        };

        let mut desc = RenderPipelineDescriptor {
            label: Some("hephae_pipeline_descriptor".into()),
            layout: [self.view_layout.clone()].into(),
            push_constant_ranges: Vec::new(),
            vertex: VertexState {
                shader: Handle::Weak(view_key.shader),
                shader_defs: defs.clone(),
                entry_point: "vertex".into(),
                buffers: [VertexBufferLayout {
                    array_stride: size_of::<T>() as BufferAddress,
                    step_mode: VertexStepMode::Vertex,
                    attributes: T::LAYOUT.into(),
                }]
                .into(),
            },
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: T::DEPTH_FORMAT.map(|format| DepthStencilState {
                format,
                depth_write_enabled: false,
                depth_compare: CompareFunction::GreaterEqual,
                stencil: StencilState {
                    front: StencilFaceState::IGNORE,
                    back: StencilFaceState::IGNORE,
                    read_mask: 0,
                    write_mask: 0,
                },
                bias: DepthBiasState {
                    constant: 0,
                    slope_scale: 0.,
                    clamp: 0.,
                },
            }),
            multisample: MultisampleState {
                count: 1 << view_key.msaa,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            fragment: Some(FragmentState {
                shader: Handle::Weak(view_key.shader),
                shader_defs: defs,
                entry_point: "fragment".into(),
                targets: [Some(ColorTargetState {
                    format,
                    blend: Some(BlendState::ALPHA_BLENDING),
                    write_mask: ColorWrites::ALL,
                })]
                .into(),
            }),
            zero_initialize_workgroup_memory: false,
        };

        T::specialize_pipeline(key, &self.vertex_prop, &mut desc);
        desc
    }
}

/// Bind group associated with each views.
#[derive(Component)]
pub struct HephaeViewBindGroup<T: Vertex>(BindGroup, PhantomData<fn() -> T>);

/// Vertex and index buffers associated with each extracted views.
#[derive(Component)]
pub struct HephaeBatch<T: Vertex> {
    vertices: RawBufferVec<T>,
    indices: RawBufferVec<u32>,
}

/// Sprite batch rendering section and [additional property](Vertex::BatchProp) for rendering.
#[derive(Component)]
#[component(storage = "SparseSet")]
pub struct HephaeBatchSection<T: Vertex> {
    prop: T::BatchProp,
    range: Range<u32>,
}

impl<T: Vertex> HephaeBatchSection<T> {
    /// Returns the user-defined property of this batch. For example, this may be used to reference
    /// a texture atlas page image for sampling.
    #[inline]
    pub fn prop(&self) -> &T::BatchProp {
        &self.prop
    }

    /// Index buffer range of the batch, for use in [`TrackedRenderPass::draw_indexed`].
    #[inline]
    pub fn range(&self) -> &Range<u32> {
        &self.range
    }
}

/// Keeps track of entities containing [`HephaeBatchSection`]. This is used instead of a [`Query`]
/// to avoid iteration on sparse set containers.
#[derive(Resource)]
pub struct HephaeBatchEntities<T: Vertex> {
    entities: Vec<Entity>,
    _marker: PhantomData<fn() -> T>,
}

impl<T: Vertex> Default for HephaeBatchEntities<T> {
    #[inline]
    fn default() -> Self {
        Self {
            entities: Vec::new(),
            _marker: PhantomData,
        }
    }
}

/// Inserts or clears vertex and index buffers associated with views.
pub fn clear_batches<T: Vertex>(
    mut commands: Commands,
    mut views: Query<(Entity, Option<&mut HephaeBatch<T>>), With<ExtractedView>>,
    mut old_batches: ResMut<HephaeBatchEntities<T>>,
) {
    for (view, batch) in &mut views {
        if let Some(mut batch) = batch {
            batch.vertices.clear();
            batch.indices.clear();
        } else {
            commands.entity(view).insert(HephaeBatch::<T> {
                vertices: RawBufferVec::new(BufferUsages::VERTEX),
                indices: RawBufferVec::new(BufferUsages::INDEX),
            });
        }
    }

    for e in old_batches.entities.drain(..) {
        if let Some(mut e) = commands.get_entity(e) {
            e.remove::<HephaeBatchSection<T>>();
        }
    }
}

/// Collects each [`VertexCommand`]s from [`VertexQueues`] into intersecting views for sorting.
pub fn queue_vertices<T: Vertex>(
    mut queues: ResMut<VertexQueues<T>>,
    draw_functions: Res<DrawFunctions<T::Item>>,
    pipeline: Res<HephaePipeline<T>>,
    shader: Res<PipelineShader<T>>,
    mut pipelines: ResMut<SpecializedRenderPipelines<HephaePipeline<T>>>,
    pipeline_cache: Res<PipelineCache>,
    mut transparent_phases: ResMut<ViewSortedRenderPhases<T::Item>>,
    views: Query<(Entity, &ExtractedView, &Msaa, Option<&Tonemapping>, Option<&DebandDither>)>,
) where
    <T::RenderCommand as RenderCommand<T::Item>>::Param: ReadOnlySystemParam,
{
    let queues = &mut *queues;
    let draw_function = draw_functions.read().id::<DrawRequests<T>>();

    for (view_entity, view, &msaa, tonemapping, dither) in &views {
        let Some(transparent_phase) = transparent_phases.get_mut(&view_entity) else {
            continue;
        };

        let view_key = ViewKey {
            hdr: view.hdr,
            msaa: msaa.samples().trailing_zeros() as u8,
            tonemapping: (!view.hdr).then_some(tonemapping.copied()).flatten(),
            dither: !view.hdr && dither.copied().unwrap_or_default() == DebandDither::Enabled,
            shader: shader.0.id(),
        };

        let Some(mut entities) = queues.entities.get_mut(&view_entity) else {
            continue;
        };

        for (e, main_e) in entities.drain() {
            let Some(commands) = queues.commands.get(&e) else { continue };
            for (i, &(layer, ref key, ..)) in commands.iter().enumerate() {
                transparent_phase.add(T::create_item(
                    layer,
                    (e, main_e),
                    pipelines.specialize(&pipeline_cache, &pipeline, (view_key, key.clone())),
                    draw_function,
                    i,
                ));
            }
        }
    }

    let bits = queues.entity_bits.get_mut().unwrap_or_else(PoisonError::into_inner);
    queues.commands.retain(|&e, _| bits.contains(e.index() as usize));
    queues.entities.iter_mut().for_each(|mut entities| entities.clear());
    bits.clear();
}

/// Accumulates sorted [`VertexCommand`]s in each view into their actual respective vertex and index
/// buffers, which are then passed to the GPU.
pub fn prepare_batch<T: Vertex>(
    mut param_set: ParamSet<(
        (
            Res<VertexQueues<T>>,
            Res<RenderDevice>,
            Res<RenderQueue>,
            ResMut<ViewSortedRenderPhases<T::Item>>,
            Query<(Entity, &mut HephaeBatch<T>), With<ExtractedView>>,
        ),
        T::BatchParam,
        (Commands, ResMut<HephaeBatchEntities<T>>),
    )>,
    mut batched_entities: Local<Vec<(Entity, T::PipelineKey, Range<u32>)>>,
    mut batched_results: Local<Vec<(Entity, HephaeBatchSection<T>)>>,
) {
    struct Queuer<'a, T: Vertex> {
        len: u32,
        vertices: &'a mut Vec<T>,
        indices: &'a mut Vec<u32>,
    }

    impl<T: Vertex> VertexQueuer for Queuer<'_, T> {
        type Vertex = T;

        #[inline]
        fn vertices(&mut self, vertices: impl IntoIterator<Item = Self::Vertex>) {
            self.vertices.extend(vertices);
        }

        #[inline]
        fn indices(&mut self, indices: impl IntoIterator<Item = u32>) {
            self.indices.extend(indices.into_iter().map(|index| index + self.len));
        }
    }

    let (queues, render_device, render_queue, mut transparent_phases, mut views) = param_set.p0();
    for (view, batch) in &mut views {
        let Some(transparent_phase) = transparent_phases.get_mut(&view) else {
            continue;
        };

        let batch = batch.into_inner();
        let mut batch_item_index = 0;
        let mut batch_index_range = 0;
        let mut batch_key = None::<T::PipelineKey>;

        let mut queuer = Queuer {
            len: 0,
            vertices: batch.vertices.values_mut(),
            indices: batch.indices.values_mut(),
        };

        for item_index in 0..transparent_phase.items.len() {
            let item = &mut transparent_phase.items[item_index];
            let Some(commands) = queues.commands.get(&item.entity()) else {
                batch_key = None;
                continue;
            };

            let Some((.., key, command)) = commands
                .get(std::mem::replace(item.batch_range_and_extra_index_mut().1, PhaseItemExtraIndex::NONE).0 as usize)
            else {
                continue;
            };

            command.draw(&mut queuer);
            queuer.len = queuer.vertices.len() as u32;

            if match batch_key {
                None => true,
                Some(ref batch_key) => batch_key != key,
            } {
                batch_item_index = item_index;
                batched_entities.push((item.entity(), key.clone(), batch_index_range..batch_index_range));
            }

            batch_index_range = queuer.indices.len() as u32;
            transparent_phase.items[batch_item_index].batch_range_mut().end += 1;
            batched_entities.last_mut().unwrap().2.end = batch_index_range;

            batch_key = Some(key.clone());
        }

        batch.vertices.write_buffer(&render_device, &render_queue);
        batch.indices.write_buffer(&render_device, &render_queue);
    }

    queues.commands.iter_mut().for_each(|mut commands| commands.clear());
    batched_results.reserve(batched_entities.len());

    let mut param = param_set.p1();
    for (batch_entity, key, range) in batched_entities.drain(..) {
        batched_results.push((batch_entity, HephaeBatchSection {
            prop: T::create_batch(&mut param, key),
            range,
        }));
    }

    drop(param);

    let (mut commands, mut batches) = param_set.p2();
    for (batch_entity, batch) in batched_results.drain(..) {
        // The batch section components are then removed in the next frame by `clear_batches`.
        commands.entity(batch_entity).insert(batch);
        batches.entities.push(batch_entity);
    }
}

/// Assigns [`HephaeViewBindGroup`]s into each views.
pub fn prepare_view_bind_groups<T: Vertex>(
    mut commands: Commands,
    pipeline: Res<HephaePipeline<T>>,
    render_device: Res<RenderDevice>,
    view_uniforms: Res<ViewUniforms>,
    views: Query<(Entity, &Tonemapping), With<ExtractedView>>,
    tonemapping_luts: Res<TonemappingLuts>,
    images: Res<RenderAssets<GpuImage>>,
    fallback_image: Res<FallbackImage>,
) {
    let Some(view_binding) = view_uniforms.uniforms.binding() else {
        return;
    };

    for (entity, &tonemapping) in &views {
        let (lut_texture, lut_sampler) = get_lut_bindings(&images, &tonemapping_luts, &tonemapping, &fallback_image);
        let view_bind_group = render_device.create_bind_group("hephae_view_bind_group", &pipeline.view_layout, &[
            BindGroupEntry {
                binding: 0,
                resource: view_binding.clone(),
            },
            BindGroupEntry {
                binding: 1,
                resource: BindingResource::TextureView(lut_texture),
            },
            BindGroupEntry {
                binding: 2,
                resource: BindingResource::Sampler(lut_sampler),
            },
        ]);

        commands
            .entity(entity)
            .insert(HephaeViewBindGroup::<T>(view_bind_group, PhantomData));
    }
}

/// Render command for drawing each vertex batches.
pub type DrawRequests<T> = (
    SetItemPipeline,
    SetHephaeViewBindGroup<T, 0>,
    <T as Vertex>::RenderCommand,
    DrawBatch<T>,
);

/// Binds the [view bind group](HephaeViewBindGroup) to `@group(I)`.
pub struct SetHephaeViewBindGroup<T: Vertex, const I: usize>(PhantomData<fn() -> T>);
impl<P: PhaseItem, T: Vertex, const I: usize> RenderCommand<P> for SetHephaeViewBindGroup<T, I> {
    type Param = ();
    type ViewQuery = (Read<ViewUniformOffset>, Read<HephaeViewBindGroup<T>>);
    type ItemQuery = ();

    #[inline]
    fn render<'w>(
        _: &P,
        (view_uniform, view_bind_group): ROQueryItem<'w, Self::ViewQuery>,
        _: Option<ROQueryItem<'w, Self::ItemQuery>>,
        _: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        pass.set_bind_group(I, &view_bind_group.0, &[view_uniform.offset]);
        RenderCommandResult::Success
    }
}

/// Renders each sprite batch entities.
pub struct DrawBatch<T: Vertex>(PhantomData<fn() -> T>);
impl<P: PhaseItem, T: Vertex> RenderCommand<P> for DrawBatch<T> {
    type Param = ();
    type ViewQuery = Read<HephaeBatch<T>>;
    type ItemQuery = Read<HephaeBatchSection<T>>;

    #[inline]
    fn render<'w>(
        _: &P,
        view: ROQueryItem<'w, Self::ViewQuery>,
        entity: Option<ROQueryItem<'w, Self::ItemQuery>>,
        _: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let Some(HephaeBatchSection { range, .. }) = entity else {
            return RenderCommandResult::Skip;
        };

        pass.set_vertex_buffer(0, view.vertices.buffer().unwrap().slice(..));
        pass.set_index_buffer(view.indices.buffer().unwrap().slice(..), 0, IndexFormat::Uint32);
        pass.draw_indexed(range.clone(), 0, 0..1);

        RenderCommandResult::Success
    }
}
