use std::{f32::consts::PI, iter::repeat_with};

use bevy::{
    core_pipeline::{bloom::Bloom, core_2d::Transparent2d},
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    ecs::{
        query::{QueryItem, ROQueryItem},
        system::{
            SystemParamItem,
            lifetimeless::{Read, SRes, SResMut},
        },
    },
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
use fastrand::Rng;
use hephae::prelude::*;

#[derive(VertexLayout, Copy, Clone, Pod, Zeroable)]
#[bytemuck(crate = "hephae::render::bytemuck")]
#[repr(C)]
struct Vert {
    #[attrib(Pos2d)]
    pos: Vec2,
    #[attrib(Uv)]
    uv: Vec2,
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
    type RenderCommand = SetSpriteBindGroup<1>;

    const SHADER: &'static str = "sprite.wgsl";

    #[inline]
    fn init_pipeline(render_device: SystemParamItem<Self::PipelineParam>) -> Self::PipelineProp {
        render_device.create_bind_group_layout("sprite_material_layout", &[
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

struct SetSpriteBindGroup<const I: usize>;
impl<P: PhaseItem, const I: usize> RenderCommand<P> for SetSpriteBindGroup<I> {
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

#[derive(TypePath, Component, Copy, Clone, Default)]
struct DrawSprite {
    pos: Vec3,
    scl: Vec2,
    page: AssetId<Image>,
    page_size: UVec2,
    rect: URect,
}

impl Drawer for DrawSprite {
    type Vertex = Vert;

    type ExtractParam = ();
    type ExtractData = (Read<GlobalTransform>, Read<AtlasCaches>);
    type ExtractFilter = ();

    type DrawParam = ();

    #[inline]
    fn extract(
        mut drawer: DrawerExtract<Self>,
        _: &SystemParamItem<Self::ExtractParam>,
        (&trns, cache): QueryItem<Self::ExtractData>,
    ) {
        let Some(&AtlasInfo {
            page, page_size, rect, ..
        }) = cache.get(0)
        else {
            return
        };

        let (scale, .., translation) = trns.to_scale_rotation_translation();
        *drawer.get_or_default() = DrawSprite {
            pos: translation,
            scl: scale.truncate(),
            page,
            page_size,
            rect,
        };
    }

    #[inline]
    fn draw(&self, _: &SystemParamItem<Self::DrawParam>, queuer: &impl VertexQueuer<Vertex = Self::Vertex>) {
        Shaper::new()
            .rect(self.pos.truncate(), self.rect.size().as_vec2() * self.scl)
            .uv_rect(self.rect, self.page_size)
            .queue_rect(queuer, self.pos.z, self.page)
    }
}

fn main() -> AppExit {
    App::new()
        .add_plugins((
            DefaultPlugins.set(ImagePlugin::default_nearest()),
            LogDiagnosticsPlugin::default(),
            FrameTimeDiagnosticsPlugin::default(),
            hephae! {
                atlas,
                render: (Vert, DrawSprite),
            },
        ))
        .add_systems(Startup, startup)
        .add_systems(Update, move_sprites)
        .run()
}

#[derive(Component, Copy, Clone, Deref, DerefMut)]
struct Velocity(Vec2);

fn startup(
    mut commands: Commands,
    server: Res<AssetServer>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut rng: Local<Rng>,
) {
    commands.spawn((
        Camera2d,
        Camera {
            clear_color: ClearColorConfig::Custom(Color::NONE),
            hdr: true,
            ..default()
        },
        Bloom::NATURAL,
    ));

    let mut z = 0f32;
    let size = window.size();
    commands.spawn_batch(
        repeat_with(|| {
            (
                Transform {
                    translation: (size * vec2(rng.f32() - 0.5, rng.f32() - 0.5)).extend({
                        let nz = z.next_up();
                        z = nz;
                        nz
                    }),
                    scale: Vec3::splat(0.2 + rng.f32() * 1.8),
                    ..default()
                },
                Velocity(Vec2::from_angle((rng.f32() * 2. - 1.) * PI) * (1. + rng.f32() * 4.)),
                AtlasEntry::new(server.load("sprites/sprites.atlas.ron"), "cix"),
                DrawBy::<DrawSprite>::new(),
            )
        })
        .take(100_000)
        .collect::<Box<_>>(),
    );
}

fn move_sprites(
    window: Single<&Window, With<PrimaryWindow>>,
    time: Res<Time>,
    mut query: Query<(&mut Transform, &mut Velocity)>,
) {
    let bl = window.size() * -0.5;
    let tr = window.size() * 0.5;

    let delta = time.delta_secs() * 60.;
    query.par_iter_mut().for_each(|(mut trns, mut vel)| {
        let mut pos = trns.translation.truncate();
        pos += **vel * delta;

        if pos.x < bl.x {
            pos.x = bl.x;
            vel.x *= -1.;
        } else if tr.x < pos.x {
            pos.x = tr.x;
            vel.x *= -1.;
        }

        if pos.y < bl.y {
            pos.y = bl.y;
            vel.y *= -1.;
        } else if tr.y < pos.y {
            pos.y = tr.y;
            vel.y *= -1.;
        }

        trns.translation = pos.extend(trns.translation.z);
    })
}
