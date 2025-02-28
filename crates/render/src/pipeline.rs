//! Provides all the necessary resources for a working base rendering pipeline.
//!
//! The procedures are as following:
//! - During [extraction](ExtractSchedule), the [pipeline shader](Vertex::SHADER) [id](AssetId) is
//!   synchronized from the main world to the render world.
//! - During [phase item queueing](bevy_render::RenderSet::Queue), each visible
//!   [drawers](crate::drawer::Drawer) queue [vertices](crate::vertex::Vertex) and indices as draw
//!   requests.
//! - During [GPU resource preparation](bevy_render::RenderSet::PrepareBindGroups), camera view bind
//!   groups are created, and for each camera view, index buffers are generated based on drawers
//!   that overlap the camera view bounds. Notably, all cameras share the same vertex buffer.
//!   Compatible vertex commands are batched; i.e., they share a section in the vertex and index
//!   buffers and share GPU render calls.
//! - [`DrawRequests`] renders each batch.

use std::{marker::PhantomData, ops::Range, sync::PoisonError};

use bevy_asset::prelude::*;
use bevy_core_pipeline::tonemapping::{
    DebandDither, Tonemapping, TonemappingLuts, get_lut_bind_group_layout_entries, get_lut_bindings,
};
use bevy_ecs::{
    entity::EntityHashMap,
    prelude::*,
    query::ROQueryItem,
    system::{
        StaticSystemParam, SystemParamItem, SystemState,
        lifetimeless::{Read, SRes},
    },
};
use bevy_image::BevyDefault;
use bevy_render::{
    Extract,
    prelude::*,
    render_asset::RenderAssets,
    render_phase::{
        DrawFunctions, PhaseItem, PhaseItemExtraIndex, RenderCommand, RenderCommandResult, SetItemPipeline,
        TrackedRenderPass, ViewSortedRenderPhases,
    },
    render_resource::{
        BindGroup, BindGroupEntry, BindGroupLayout, BindingResource, BlendState, Buffer, BufferAddress, BufferBinding,
        BufferDescriptor, BufferId, BufferInitDescriptor, BufferSize, BufferUsages, ColorTargetState, ColorWrites,
        CompareFunction, DepthBiasState, DepthStencilState, FragmentState, FrontFace, IndexFormat, MultisampleState,
        PipelineCache, PolygonMode, PrimitiveState, PrimitiveTopology, RenderPipelineDescriptor, SamplerId, ShaderDefVal,
        ShaderStages, ShaderType, SpecializedRenderPipeline, SpecializedRenderPipelines, StencilFaceState, StencilState,
        TextureFormat, TextureViewId, VertexBufferLayout, VertexState, VertexStepMode, binding_types::uniform_buffer,
    },
    renderer::{RenderDevice, RenderQueue},
    sync_world::MainEntity,
    texture::{FallbackImage, GpuImage},
    view::{ExtractedView, ViewTarget, ViewUniform, ViewUniformOffset, ViewUniforms},
};
use bevy_utils::default;
use bytemuck::cast_slice;
use fixedbitset::FixedBitSet;
use vec_belt::VecBelt;

use crate::vertex::{DrawItems, Vertex};

/// Common pipeline descriptor for use in [specialization](Vertex::specialize_pipeline). See the
/// module-level documentation.
#[derive(Resource)]
pub struct VertexPipeline<T: Vertex> {
    view_layout: BindGroupLayout,
    vertex_prop: T::PipelineProp,
}

impl<T: Vertex> VertexPipeline<T> {
    /// Returns the [additional property](Vertex::PipelineProp) of the vertex definition for use in
    /// [specialization](Vertex::specialize_pipeline).
    #[inline]
    pub const fn vertex_prop(&self) -> &T::PipelineProp {
        &self.vertex_prop
    }
}

impl<T: Vertex> FromWorld for VertexPipeline<T> {
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

impl<T: Vertex> SpecializedRenderPipeline for VertexPipeline<T> {
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
                    attributes: T::ATTRIBUTES.into(),
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

/// Global vertex buffer written to by [`Drawer`](crate::drawer::Drawer)s in parallel.
#[derive(Resource)]
pub struct DrawBuffers<T: Vertex> {
    pub(crate) vertices: VecBelt<T>,
    pub(crate) indices: VecBelt<u32>,
    vertex_buffer: Buffer,
}

impl<T: Vertex> FromWorld for DrawBuffers<T> {
    #[inline]
    fn from_world(world: &mut World) -> Self {
        let device = world.resource::<RenderDevice>();
        Self {
            vertices: VecBelt::new(4096),
            indices: VecBelt::new(6144),
            vertex_buffer: device.create_buffer(&BufferDescriptor {
                label: Some("hephae_vertex_buffer"),
                size: (4096 * size_of::<T>()) as BufferAddress,
                usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
        }
    }
}

/// Render phase items associated with each views that are responsible over batching draw calls.
#[derive(Component)]
pub struct ViewBatches<T: Vertex>(pub EntityHashMap<(T::BatchProp, Range<u32>)>);
impl<T: Vertex> Default for ViewBatches<T> {
    #[inline]
    fn default() -> Self {
        Self(default())
    }
}

/// Bind group associated with each views.
#[derive(Component)]
pub struct ViewBindGroup<T: Vertex> {
    bind_group: BindGroup,
    last_buffer: BufferId,
    last_lut_texture: TextureViewId,
    last_lut_sampler: SamplerId,
    _marker: PhantomData<fn() -> T>,
}

/// Index buffer associated with each views.
#[derive(Component)]
pub struct ViewIndexBuffer<T: Vertex> {
    indices: Vec<u32>,
    index_buffer: Option<Buffer>,
    _marker: PhantomData<fn() -> T>,
}

impl<T: Vertex> Default for ViewIndexBuffer<T> {
    #[inline]
    fn default() -> Self {
        Self {
            indices: Vec::with_capacity(6144),
            index_buffer: None,
            _marker: PhantomData,
        }
    }
}

#[derive(Component)]
pub(crate) struct VisibleDrawers<T: Vertex>(pub VecBelt<Entity>, PhantomData<fn() -> T>);
impl<T: Vertex> Default for VisibleDrawers<T> {
    #[inline]
    fn default() -> Self {
        Self(VecBelt::new(1024), PhantomData)
    }
}

pub(crate) fn queue_vertices<T: Vertex>(
    draw_functions: Res<DrawFunctions<T::Item>>,
    pipeline: Res<VertexPipeline<T>>,
    shader: Res<PipelineShader<T>>,
    mut pipelines: ResMut<SpecializedRenderPipelines<VertexPipeline<T>>>,
    pipeline_cache: Res<PipelineCache>,
    mut transparent_phases: ResMut<ViewSortedRenderPhases<T::Item>>,
    mut views: Query<(
        Entity,
        &mut VisibleDrawers<T>,
        &ExtractedView,
        &Msaa,
        Option<&Tonemapping>,
        Option<&DebandDither>,
    )>,
    mut items: Query<(Entity, &MainEntity, &mut DrawItems<T>)>,
    mut iterated: Local<FixedBitSet>,
) {
    let draw_function = draw_functions.read().id::<DrawRequests<T>>();
    for item in &mut views {
        let (view_entity, mut visible_drawers, view, &msaa, tonemapping, dither): (
            Entity,
            Mut<VisibleDrawers<T>>,
            &ExtractedView,
            &Msaa,
            Option<&Tonemapping>,
            Option<&DebandDither>,
        ) = item;

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

        iterated.clear();
        visible_drawers.0.clear(|entities| {
            let mut iter = items.iter_many_mut(entities);
            while let Some((e, &main_e, mut items)) = iter.fetch_next() {
                let index = e.index() as usize;
                if iterated[index] {
                    continue;
                }

                iterated.grow_and_insert(index);
                transparent_phase.items.extend(
                    items
                        .0
                        .get_mut()
                        .unwrap_or_else(PoisonError::into_inner)
                        .iter_mut()
                        .enumerate()
                        .map(|(i, &mut (.., layer, ref key))| {
                            T::create_item(
                                layer,
                                (e, main_e),
                                pipelines.specialize(&pipeline_cache, &pipeline, (view_key, key.clone())),
                                draw_function,
                                i,
                            )
                        }),
                );
            }
        });
    }
}

pub(crate) fn prepare_indices<T: Vertex>(
    mut param_set: ParamSet<(
        (
            Res<RenderDevice>,
            Res<RenderQueue>,
            ResMut<DrawBuffers<T>>,
            ResMut<ViewSortedRenderPhases<T::Item>>,
            Query<(Entity, &mut ViewIndexBuffer<T>)>,
            Query<&mut DrawItems<T>>,
        ),
        StaticSystemParam<T::BatchParam>,
        Query<&mut ViewBatches<T>>,
    )>,
    mut batched_entities: Local<Vec<(Entity, Entity, T::PipelineKey, Range<u32>)>>,
    mut batched_results: Local<Vec<(Entity, Entity, T::BatchProp, Range<u32>)>>,
) {
    let (device, queue, buffers, mut transparent_phases, mut views, mut items): (
        Res<RenderDevice>,
        Res<RenderQueue>,
        ResMut<DrawBuffers<T>>,
        ResMut<ViewSortedRenderPhases<T::Item>>,
        Query<(Entity, &mut ViewIndexBuffer<T>)>,
        Query<&mut DrawItems<T>>,
    ) = param_set.p0();

    batched_entities.clear();

    let buffers = buffers.into_inner();
    buffers.vertices.clear(|vertices| {
        let contents = cast_slice::<T, u8>(&vertices);
        if (buffers.vertex_buffer.size() as usize) < contents.len() {
            buffers.vertex_buffer = device.create_buffer_with_data(&BufferInitDescriptor {
                label: Some("hephae_vertex_buffer"),
                contents,
                usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            });
        } else if let Some(len) = BufferSize::new(contents.len() as u64) {
            queue
                .write_buffer_with(&buffers.vertex_buffer, 0, len)
                .unwrap()
                .copy_from_slice(contents);
        }
    });

    buffers.indices.clear(|indices| {
        for (view_entity, view_indices) in &mut views {
            let Some(transparent_phase) = transparent_phases.get_mut(&view_entity) else {
                continue;
            };

            let view_indices = view_indices.into_inner();
            view_indices.indices.clear();

            let mut batch_item_index = 0;
            let mut batch_index_range = 0;
            let mut batch_key = None::<T::PipelineKey>;

            for item_index in 0..transparent_phase.items.len() {
                let item = &mut transparent_phase.items[item_index];
                let Ok(mut items) = items.get_mut(item.entity()) else {
                    batch_key = None;
                    continue;
                };

                let Some((range, .., key)) =
                    items.0.get_mut().unwrap_or_else(PoisonError::into_inner).get(
                        std::mem::replace(item.batch_range_and_extra_index_mut().1, PhaseItemExtraIndex::NONE).0 as usize,
                    )
                else {
                    continue;
                };

                view_indices.indices.extend(&indices[range.clone()]);
                if match batch_key {
                    None => true,
                    Some(ref batch_key) => batch_key != key,
                } {
                    batch_item_index = item_index;
                    batched_entities.push((view_entity, item.entity(), key.clone(), batch_index_range..batch_index_range));
                }

                batch_index_range = view_indices.indices.len() as u32;
                transparent_phase.items[batch_item_index].batch_range_mut().end += 1;
                batched_entities.last_mut().unwrap().3.end = batch_index_range;

                batch_key = Some(key.clone());
            }

            let contents = cast_slice::<u32, u8>(&view_indices.indices);
            if view_indices
                .index_buffer
                .as_ref()
                .is_none_or(|index_buffer| (index_buffer.size() as usize) < contents.len())
            {
                view_indices.index_buffer = Some(device.create_buffer_with_data(&BufferInitDescriptor {
                    label: Some("hephae_index_buffer"),
                    contents,
                    usage: BufferUsages::INDEX | BufferUsages::COPY_DST,
                }));
            } else if let Some(len) = BufferSize::new(contents.len() as u64) {
                queue
                    .write_buffer_with(view_indices.index_buffer.as_ref().unwrap(), 0, len)
                    .unwrap()
                    .copy_from_slice(contents);
            }
        }
    });

    for mut item in &mut items {
        item.0.get_mut().unwrap_or_else(PoisonError::into_inner).clear();
    }

    let mut param = param_set.p1();
    batched_results.extend(batched_entities.drain(..).map(|(view_entity, batch_entity, key, range)| {
        (view_entity, batch_entity, T::create_batch(&mut param, key), range)
    }));

    drop(param);

    let mut batches = param_set.p2();
    for mut view_batches in &mut batches {
        view_batches.0.clear();
    }

    for (view_entity, batch_entity, prop, range) in batched_results.drain(..) {
        let Ok(mut view_batches) = batches.get_mut(view_entity) else {
            continue;
        };

        view_batches.0.insert(batch_entity, (prop, range));
    }
}

/// Assigns [`ViewBindGroup`]s into each views.
pub(crate) fn prepare_view_bind_groups<T: Vertex>(
    mut commands: Commands,
    pipeline: Res<VertexPipeline<T>>,
    render_device: Res<RenderDevice>,
    view_uniforms: Res<ViewUniforms>,
    mut views: Query<(Entity, &Tonemapping, Option<&mut ViewBindGroup<T>>)>,
    tonemapping_luts: Res<TonemappingLuts>,
    images: Res<RenderAssets<GpuImage>>,
    fallback_image: Res<FallbackImage>,
) {
    let Some(buffer) = view_uniforms.uniforms.buffer() else {
        return;
    };

    let view_binding = BindingResource::Buffer(BufferBinding {
        buffer,
        offset: 0,
        size: Some(ViewUniform::min_size()),
    });

    for (entity, &tonemapping, bind_group) in &mut views {
        let (lut_texture, lut_sampler) = get_lut_bindings(&images, &tonemapping_luts, &tonemapping, &fallback_image);
        let create_bind_group = || ViewBindGroup::<T> {
            bind_group: render_device.create_bind_group("hephae_view_bind_group", &pipeline.view_layout, &[
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
            ]),
            last_buffer: buffer.id(),
            last_lut_texture: lut_texture.id(),
            last_lut_sampler: lut_sampler.id(),
            _marker: PhantomData,
        };

        if let Some(mut bind_group) = bind_group {
            if bind_group.last_buffer != buffer.id() ||
                bind_group.last_lut_texture != lut_texture.id() ||
                bind_group.last_lut_sampler != lut_sampler.id()
            {
                *bind_group = create_bind_group();
            }
        } else {
            commands.entity(entity).insert(create_bind_group());
        }
    }
}

/// Render command for drawing each vertex batches.
pub type DrawRequests<T> = (
    SetItemPipeline,
    SetViewBindGroup<T, 0>,
    <T as Vertex>::RenderCommand,
    DrawBatch<T>,
);

/// Binds the [view bind group](ViewBindGroup) to `@group(I)`.
pub struct SetViewBindGroup<T: Vertex, const I: usize>(PhantomData<fn() -> T>);
impl<P: PhaseItem, T: Vertex, const I: usize> RenderCommand<P> for SetViewBindGroup<T, I> {
    type Param = ();
    type ViewQuery = (Read<ViewUniformOffset>, Read<ViewBindGroup<T>>);
    type ItemQuery = ();

    #[inline]
    fn render<'w>(
        _: &P,
        (view_uniform, view_bind_group): ROQueryItem<'w, Self::ViewQuery>,
        _: Option<ROQueryItem<'w, Self::ItemQuery>>,
        _: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        pass.set_bind_group(I, &view_bind_group.bind_group, &[view_uniform.offset]);
        RenderCommandResult::Success
    }
}

/// Renders each sprite batch entities.
pub struct DrawBatch<T: Vertex>(PhantomData<fn() -> T>);
impl<P: PhaseItem, T: Vertex> RenderCommand<P> for DrawBatch<T> {
    type Param = SRes<DrawBuffers<T>>;
    type ViewQuery = (Read<ViewBatches<T>>, Read<ViewIndexBuffer<T>>);
    type ItemQuery = ();

    #[inline]
    fn render<'w>(
        item: &P,
        (batches, index_buffer): ROQueryItem<'w, Self::ViewQuery>,
        _: Option<ROQueryItem<'w, Self::ItemQuery>>,
        buffers: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let Some((.., range)) = batches.0.get(&item.entity()) else {
            return RenderCommandResult::Skip;
        };

        let Some(ref index_buffer) = index_buffer.index_buffer else {
            return RenderCommandResult::Skip;
        };

        let buffers = buffers.into_inner();
        pass.set_vertex_buffer(0, buffers.vertex_buffer.slice(..));
        pass.set_index_buffer(index_buffer.slice(..), 0, IndexFormat::Uint32);
        pass.draw_indexed(range.clone(), 0, 0..1);

        RenderCommandResult::Success
    }
}
