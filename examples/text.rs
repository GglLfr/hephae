use bevy::{
    core_pipeline::core_2d::Transparent2d,
    ecs::{
        query::{QueryItem, ROQueryItem},
        system::{
            SystemParamItem,
            lifetimeless::{Read, SRes, SResMut},
        },
    },
    math::vec3,
    prelude::*,
    render::{
        render_asset::RenderAssets,
        render_phase::{PhaseItem, RenderCommand, RenderCommandResult, TrackedRenderPass},
        render_resource::{
            BindGroupEntry, BindGroupLayout, IntoBinding, RenderPipelineDescriptor, SamplerBindingType, ShaderStages,
            TextureSampleType,
            binding_types::{sampler, texture_2d},
        },
        renderer::RenderDevice,
        texture::GpuImage,
        view::ExtractedView,
    },
    window::PrimaryWindow,
};
use hephae::{locale::def::LocaleChangeEvent, prelude::*, text::atlas::FontAtlas};

#[derive(VertexLayout, Copy, Clone, Pod, Zeroable)]
#[bytemuck(crate = "hephae::render::bytemuck")]
#[repr(C)]
struct Vert {
    #[attrib(Pos2d)]
    pos: Vec2,
    #[attrib(Uv)]
    uv: Vec2,
    #[attrib(ByteColor)]
    col: [Nor<u8>; 4],
}

impl Vertex for Vert {
    type PipelineParam = SRes<RenderDevice>;
    type PipelineProp = BindGroupLayout;
    type PipelineKey = AssetId<Image>;

    type BatchParam = (
        SRes<RenderDevice>,
        SRes<RenderAssets<GpuImage>>,
        SRes<VertexPipeline<Self>>,
        SResMut<ImageBindGroups>,
    );
    type BatchProp = AssetId<Image>;

    type Item = Transparent2d;
    type RenderCommand = SetTextBindGroup<1>;

    const SHADER: &'static str = "text.wgsl";

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

    #[inline]
    fn create_batch(
        (render_device, gpu_images, pipeline, image_bind_groups): &mut SystemParamItem<Self::BatchParam>,
        key: Self::PipelineKey,
    ) -> Self::BatchProp {
        let Some(gpu_image) = gpu_images.get(key) else {
            return key;
        };
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
    type Param = (SRes<ImageBindGroups>, SRes<ViewBatches<Vert>>);
    type ViewQuery = Read<ExtractedView>;
    type ItemQuery = ();

    #[inline]
    fn render<'w>(
        item: &P,
        view: ROQueryItem<'w, Self::ViewQuery>,
        _: Option<ROQueryItem<'w, Self::ItemQuery>>,
        (image_bind_groups, batches): SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let image_bind_groups = image_bind_groups.into_inner();
        let Some(&(id, ..)) = batches.get(&(view.retained_view_entity, item.entity())) else {
            return RenderCommandResult::Skip;
        };

        let Some(bind_group) = image_bind_groups.get(id) else {
            return RenderCommandResult::Skip;
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
    fn draw(&self, atlases: &SystemParamItem<Self::DrawParam>, queuer: &impl VertexQueuer<Vertex = Self::Vertex>) {
        for &glyph in &self.glyphs {
            let Some(atlas) = atlases.get(glyph.atlas) else {
                continue;
            };
            let Some((.., rect)) = atlas.get_info_index(glyph.index) else {
                continue;
            };

            Shaper::new()
                .rect_bl(self.pos + glyph.origin, rect.size().as_vec2())
                .uv_rect(rect, atlas.size())
                .byte_color([127, 255, 100, 255].map(Nor))
                .queue_rect(queuer, 0., atlas.image())
        }
    }
}

fn main() -> AppExit {
    App::new()
        .add_plugins((DefaultPlugins.set(ImagePlugin::default_nearest()), hephae! {
            render: (Vert, DrawText),
            locale,
            text,
        }))
        .add_systems(Startup, startup)
        .add_systems(Update, (move_camera, update, switch_locale))
        .run()
}

fn startup(mut commands: Commands, server: Res<AssetServer>) {
    commands.spawn((Camera2d, Msaa::Off));

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
            DrawBy::<DrawText>::new(),
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
    window: Single<&Window, With<PrimaryWindow>>,
    fonts: Res<Assets<Font>>,
    mut images: ResMut<Assets<Image>>,
    mut atlases: ResMut<Assets<FontAtlas>>,
) {
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
        events.write(LocaleChangeEvent(
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
