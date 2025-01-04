use std::mem::offset_of;

use bevy::{
    core_pipeline::core_2d::Transparent2d,
    ecs::{
        query::{QueryItem, ROQueryItem},
        system::{
            lifetimeless::{Read, SRes, SResMut},
            SystemParamItem,
        },
    },
    math::{vec2, FloatOrd},
    prelude::*,
    render::{
        render_asset::RenderAssets,
        render_phase::{
            DrawFunctionId, PhaseItem, PhaseItemExtraIndex, RenderCommand, RenderCommandResult, TrackedRenderPass,
        },
        render_resource::{
            binding_types::{sampler, texture_2d},
            BindGroupEntry, BindGroupLayout, BufferAddress, CachedRenderPipelineId, IntoBinding, RenderPipelineDescriptor,
            SamplerBindingType, ShaderStages, TextureSampleType, VertexAttribute, VertexFormat,
        },
        renderer::RenderDevice,
        sync_world::MainEntity,
        texture::GpuImage,
    },
    window::PrimaryWindow,
};
use bytemuck::{Pod, Zeroable};
use hephae::{
    prelude::*,
    render::{
        image_bind::ImageBindGroups,
        pipeline::{HephaeBatchSection, HephaePipeline},
    },
};
use hephae_text::atlas::FontAtlas;

#[derive(Copy, Clone, Pod, Zeroable)]
#[repr(C)]
struct Vert {
    pos: [f32; 2],
    uv: [f32; 2],
}

impl Vert {
    #[inline]
    pub const fn new(xy: Vec2, uv: Vec2) -> Self {
        Self {
            pos: [xy.x, xy.y],
            uv: [uv.x, uv.y],
        }
    }
}

impl Vertex for Vert {
    type PipelineParam = SRes<RenderDevice>;
    type PipelineProp = BindGroupLayout;
    type PipelineKey = AssetId<Image>;

    type Command = Glyph;

    type BatchParam = (
        SRes<RenderDevice>,
        SRes<RenderAssets<GpuImage>>,
        SRes<HephaePipeline<Self>>,
        SResMut<ImageBindGroups>,
    );
    type BatchProp = AssetId<Image>;

    type Item = Transparent2d;
    type RenderCommand = SetSpriteBindGroup<1>;

    const SHADER: &'static str = "sprite.wgsl";
    const LAYOUT: &'static [VertexAttribute] = &[
        VertexAttribute {
            format: VertexFormat::Float32x2,
            offset: offset_of!(Self, pos) as BufferAddress,
            shader_location: 0,
        },
        VertexAttribute {
            format: VertexFormat::Float32x2,
            offset: offset_of!(Self, uv) as BufferAddress,
            shader_location: 1,
        },
    ];

    #[inline]
    fn init_pipeline(render_device: SystemParamItem<Self::PipelineParam>) -> Self::PipelineProp {
        render_device.create_bind_group_layout("text_material_layout", &[
            texture_2d(TextureSampleType::Float { filterable: true }).build(0, ShaderStages::FRAGMENT),
            sampler(SamplerBindingType::Filtering).build(1, ShaderStages::FRAGMENT),
        ])
    }

    #[inline]
    fn specialize_pipeline(
        _: Self::PipelineKey,
        material_bind_group: &Self::PipelineProp,
        desc: &mut RenderPipelineDescriptor,
    ) {
        desc.layout.push(material_bind_group.clone());
    }

    fn create_item(
        layer: f32,
        entity: (Entity, MainEntity),
        pipeline: CachedRenderPipelineId,
        draw_function: DrawFunctionId,
        command: usize,
    ) -> Self::Item {
        Transparent2d {
            sort_key: FloatOrd(layer),
            entity,
            pipeline,
            draw_function,
            batch_range: 0..0,
            extra_index: PhaseItemExtraIndex(command as u32),
        }
    }

    #[inline]
    fn create_batch(
        (ref render_device, ref gpu_images, ref pipeline, image_bind_groups): &mut SystemParamItem<Self::BatchParam>,
        key: Self::PipelineKey,
    ) -> Self::BatchProp {
        let Some(gpu_image) = gpu_images.get(key) else { return key };
        image_bind_groups.create(key, render_device, pipeline.vertex_prop(), &[
            BindGroupEntry {
                binding: 0,
                resource: gpu_image.texture_view.into_binding(),
            },
            BindGroupEntry {
                binding: 1,
                resource: gpu_image.sampler.into_binding(),
            },
        ]);

        key
    }
}

struct SetSpriteBindGroup<const I: usize>;
impl<P: PhaseItem, const I: usize> RenderCommand<P> for SetSpriteBindGroup<I> {
    type Param = SRes<ImageBindGroups>;
    type ViewQuery = ();
    type ItemQuery = Read<HephaeBatchSection<Vert>>;

    #[inline]
    fn render<'w>(
        _: &P,
        _: ROQueryItem<'w, Self::ViewQuery>,
        batch: Option<ROQueryItem<'w, Self::ItemQuery>>,
        image_bind_groups: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let image_bind_groups = image_bind_groups.into_inner();
        let Some(batch) = batch else {
            return RenderCommandResult::Skip
        };

        let Some(bind_group) = image_bind_groups.get(*batch.prop()) else {
            return RenderCommandResult::Skip
        };

        pass.set_bind_group(I, bind_group, &[]);
        RenderCommandResult::Success
    }
}

#[derive(TypePath, Component, Clone)]
struct DrawText {
    pos: Vec2,
    glyphs: Vec<TextGlyph>,
}

impl Drawer for DrawText {
    type Vertex = Vert;

    type ExtractParam = ();
    type ExtractData = (Read<GlobalTransform>, Read<TextGlyphs>);
    type ExtractFilter = ();

    type DrawParam = SRes<ExtractedFontAtlases>;

    #[inline]
    fn extract(_: &SystemParamItem<Self::ExtractParam>, (&trns, glyphs): QueryItem<Self::ExtractData>) -> Option<Self> {
        Some(Self {
            pos: trns.translation().truncate() - glyphs.size / 2.,
            glyphs: glyphs.glyphs.clone(),
        })
    }

    #[inline]
    fn enqueue(
        &self,
        atlases: &SystemParamItem<Self::DrawParam>,
        queuer: &mut impl Extend<(f32, <Self::Vertex as Vertex>::PipelineKey, <Self::Vertex as Vertex>::Command)>,
    ) {
        queuer.extend(self.glyphs.iter().flat_map(|&glyph| {
            let atlas = atlases.get(glyph.atlas)?;
            let (.., rect) = atlas.get_info_index(glyph.index)?;

            Some((0., atlas.image(), Glyph {
                pos: self.pos + glyph.origin,
                rect: rect.as_rect(),
                atlas: atlas.size().as_vec2(),
            }))
        }));
    }
}

#[derive(Copy, Clone)]
struct Glyph {
    pos: Vec2,
    rect: Rect,
    atlas: Vec2,
}

impl VertexCommand for Glyph {
    type Vertex = Vert;

    #[inline]
    fn draw(&self, queuer: &mut impl VertexQueuer<Vertex = Self::Vertex>) {
        let Self { pos, rect, atlas } = *self;
        let bottom_left = (pos, vec2(rect.min.x, rect.max.y) / atlas);
        let bottom_right = (pos + vec2(rect.width(), 0.), rect.max / atlas);
        let top_right = (pos + vec2(rect.width(), rect.height()), vec2(rect.max.x, rect.min.y) / atlas);
        let top_left = (pos + vec2(0., rect.height()), rect.min / atlas);

        queuer.vertices([
            Vert::new(bottom_left.0, bottom_left.1),
            Vert::new(bottom_right.0, bottom_right.1),
            Vert::new(top_right.0, top_right.1),
            Vert::new(top_left.0, top_left.1),
        ]);

        queuer.indices([0, 1, 2, 2, 3, 0]);
    }
}

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.set(ImagePlugin::default_nearest()),
            HephaeRenderPlugin::<Vert>::new(),
            DrawerPlugin::<DrawText>::new(),
            HephaeTextPlugin,
        ))
        .add_systems(Startup, startup)
        .add_systems(Update, update)
        .run();
}

fn startup(mut commands: Commands, server: Res<AssetServer>) {
    commands.spawn(Camera2d);

    commands.spawn((
        Transform::IDENTITY,
        Text::new("Hi, Hephae!"),
        TextFont {
            font: server.load("fonts/roboto.ttf"),
            font_size: 64.,
            line_height: 1.,
            antialias: true,
        },
        HasDrawer::<DrawText>::new(),
    ));
}

fn update(
    mut font_layout: ResMut<FontLayout>,
    mut query: Query<(&mut TextGlyphs, &Text, &TextFont)>,
    window: Query<&Window, With<PrimaryWindow>>,
    fonts: Res<Assets<Font>>,
    mut images: ResMut<Assets<Image>>,
    mut atlases: ResMut<Assets<FontAtlas>>,
    mut updated: Local<bool>,
) {
    if *updated {
        return
    }

    let Ok(window) = window.get_single() else { return };
    let scale = window.scale_factor();

    for (mut glyphs, text, text_font) in &mut query {
        if font_layout
            .compute_glyphs(
                &mut glyphs,
                (None, None),
                default(),
                default(),
                scale,
                &fonts,
                &mut images,
                &mut atlases,
                [(&*text.text, text_font, LinearRgba::WHITE)].into_iter(),
            )
            .is_ok()
        {
            *updated = true;
        }
    }
}
