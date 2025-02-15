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
    math::{vec2, vec3, FloatOrd},
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
use hephae::{locale::def::LocaleChangeEvent, prelude::*, text::atlas::FontAtlas};

#[derive(Copy, Clone, Pod, Zeroable)]
#[repr(C)]
struct Vert {
    pos: [f32; 2],
    uv: [f32; 2],
    col: [u8; 4],
}

impl Vert {
    #[inline]
    pub const fn new(xy: Vec2, uv: Vec2, col: [u8; 4]) -> Self {
        Self {
            pos: [xy.x, xy.y],
            uv: [uv.x, uv.y],
            col,
        }
    }
}

impl Vertex for Vert {
    type PipelineParam = SRes<RenderDevice>;
    type PipelineProp = BindGroupLayout;
    type PipelineKey = AssetId<Image>;

    type BatchParam = (
        SRes<RenderDevice>,
        SRes<RenderAssets<GpuImage>>,
        SRes<HephaePipeline<Self>>,
        SResMut<ImageBindGroups>,
    );
    type BatchProp = AssetId<Image>;

    type Item = Transparent2d;
    type RenderCommand = SetTextBindGroup<1>;

    const SHADER: &'static str = "text.wgsl";
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
        VertexAttribute {
            format: VertexFormat::Unorm8x4,
            offset: offset_of!(Self, col) as BufferAddress,
            shader_location: 2,
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

struct SetTextBindGroup<const I: usize>;
impl<P: PhaseItem, const I: usize> RenderCommand<P> for SetTextBindGroup<I> {
    type Param = SRes<ImageBindGroups>;
    type ViewQuery = Read<ViewBatches<Vert>>;
    type ItemQuery = ();

    #[inline]
    fn render<'w>(
        item: &P,
        view: ROQueryItem<'w, Self::ViewQuery>,
        _: Option<ROQueryItem<'w, Self::ItemQuery>>,
        image_bind_groups: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let image_bind_groups = image_bind_groups.into_inner();
        let Some(&(id, ..)) = view.0.get(&item.entity()) else {
            return RenderCommandResult::Skip
        };

        let Some(bind_group) = image_bind_groups.get(id) else {
            return RenderCommandResult::Skip
        };

        pass.set_bind_group(I, bind_group, &[]);
        RenderCommandResult::Success
    }
}

#[derive(TypePath, Component, Default)]
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
    fn extract(
        mut drawer: DrawerExtract<Self>,
        _: &SystemParamItem<Self::ExtractParam>,
        (&trns, glyphs): QueryItem<Self::ExtractData>,
    ) {
        let drawer = drawer.get_or_default();
        drawer.pos = trns.translation().truncate() - glyphs.size / 2.;
        drawer.glyphs.clone_from(&glyphs.glyphs);
    }

    #[inline]
    fn draw(&mut self, atlases: &SystemParamItem<Self::DrawParam>, queuer: &impl VertexQueuer<Vertex = Self::Vertex>) {
        for glyph in self.glyphs.drain(..) {
            let Some(atlas) = atlases.get(glyph.atlas) else { continue };
            let Some((.., rect)) = atlas.get_info_index(glyph.index) else {
                continue
            };

            let pos = self.pos + glyph.origin;
            let rect = rect.as_rect();
            let atlas_size = atlas.size().as_vec2();

            let (w, h) = (rect.width(), rect.height());
            let (u, v, u2, v2) = (
                rect.min.x / atlas_size.x,
                rect.max.y / atlas_size.y,
                rect.max.x / atlas_size.x,
                rect.min.y / atlas_size.y,
            );

            let bottom_left = (pos, vec2(u, v));
            let bottom_right = (pos + vec2(w, 0.), vec2(u2, v));
            let top_right = (pos + vec2(w, h), vec2(u2, v2));
            let top_left = (pos + vec2(0., h), vec2(u, v2));

            let col = [127, 255, 100, 255];
            let base = queuer.data([
                Vert::new(bottom_left.0, bottom_left.1, col),
                Vert::new(bottom_right.0, bottom_right.1, col),
                Vert::new(top_right.0, top_right.1, col),
                Vert::new(top_left.0, top_left.1, col),
            ]);

            queuer.request(0., atlas.image(), [base, base + 1, base + 2, base + 2, base + 3, base]);
        }
    }
}

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.set(ImagePlugin::default_nearest()),
            hephae::render::<Vert, DrawText>(),
            hephae::locales::<(), ()>(),
            hephae::text(),
        ))
        .add_systems(Startup, startup)
        .add_systems(Update, (move_camera, update, switch_locale))
        .run();
}

fn startup(mut commands: Commands, server: Res<AssetServer>) {
    commands.spawn(Camera2d);

    commands.spawn_localized(
        (
            Transform::IDENTITY,
            Text {
                wrap: TextWrap::Word,
                align: TextAlign::Center,
                ..default()
            },
            TextFont {
                font: server.load("fonts/roboto.ttf"),
                font_size: 64.,
                line_height: 1.,
                antialias: true,
            },
            HasDrawer::<DrawText>::new(),
        ),
        "intro",
        server.load("locales/locales.ron"),
        LocalizeBy("user".into()),
    );
}

fn move_camera(time: Res<Time>, input: Res<ButtonInput<KeyCode>>, mut camera: Query<&mut Transform, With<Camera>>) {
    let [up, down, left, right] = [
        input.pressed(KeyCode::KeyW),
        input.pressed(KeyCode::KeyS),
        input.pressed(KeyCode::KeyA),
        input.pressed(KeyCode::KeyD),
    ]
    .map(|pressed| if pressed { 1f32 } else { 0. });

    for mut trns in &mut camera {
        trns.translation += vec3(right - left, up - down, 0.) * time.delta_secs() * 120.;
    }
}

fn update(
    mut font_layout: ResMut<FontLayout>,
    mut query: Query<(&mut TextGlyphs, &mut Text, &TextFont), Changed<TextStructure>>,
    window: Query<&Window, With<PrimaryWindow>>,
    fonts: Res<Assets<Font>>,
    mut images: ResMut<Assets<Image>>,
    mut atlases: ResMut<Assets<FontAtlas>>,
) {
    let Ok(window) = window.get_single() else { return };
    let scale = window.scale_factor();

    for (mut glyphs, mut text, text_font) in &mut query {
        if let Err(e) = font_layout.get_mut().compute_glyphs(
            &mut glyphs,
            (Some(800.), None),
            text.wrap,
            text.align,
            scale,
            &fonts,
            &mut images,
            &mut atlases,
            [(&*text.text, text_font)].into_iter(),
        ) {
            warn!("Scheduling text for update again due to: {e}");
            text.set_changed();
        }
    }
}

fn switch_locale(
    mut events: EventWriter<LocaleChangeEvent>,
    time: Res<Time>,
    mut to_english: Local<bool>,
    mut timer: Local<f64>,
) {
    *timer += time.delta_secs_f64();
    if *timer >= 1. {
        *timer %= 1.;
        events.send(LocaleChangeEvent(
            match *to_english {
                false => {
                    *to_english = true;
                    "id-ID"
                }
                true => {
                    *to_english = false;
                    "en-US"
                }
            }
            .into(),
        ));
    }
}
