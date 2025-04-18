use std::{collections::VecDeque, num::NonZeroUsize, path::PathBuf, time::Duration};

use bevy::{
    core_pipeline::{bloom::Bloom, core_2d::Transparent2d, tonemapping::Tonemapping},
    ecs::{
        query::{QueryItem, ROQueryItem},
        system::{
            SystemParamItem,
            lifetimeless::{Read, SRes, SResMut},
        },
    },
    math::VectorSpace,
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
};
use hephae::prelude::*;

#[derive(Copy, Clone, VertexLayout, Pod, Zeroable)]
#[bytemuck(crate = "hephae::render::bytemuck")]
#[repr(C)]
struct Vert {
    #[attrib(Pos2d)]
    pos: Vec2,
    #[attrib(Uv)]
    uv: Vec2,
    #[attrib(Color)]
    col: LinearRgba,
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

    const SHADER: &'static str = "trail.wgsl";

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

/// 128 ticks per second.
const TRAIL_UPDATE_RATE: Duration = Duration::new(0, 1_000_000_000 / 128);
const TRAIL_LENGTH: usize = 128;

#[derive(Component)]
struct PrimaryTrail;

#[derive(Component, Clone)]
#[require(DrawBy<DrawTrail>)]
struct Trail {
    points: VecDeque<Vec2>,
    max_len: NonZeroUsize,
    head: Vec2,
    tail: Vec2,
    still: usize,
    shrinking: Option<usize>,
}

impl Trail {
    fn new(max_len: NonZeroUsize) -> Self {
        Self {
            points: VecDeque::with_capacity(max_len.get()),
            max_len,
            head: Vec2::ZERO,
            tail: Vec2::ZERO,
            still: 0,
            shrinking: None,
        }
    }
}

#[derive(Copy, Clone)]
struct Interp<T: VectorSpace> {
    start: T,
    end: T,
    func: fn(f32) -> f32,
}

impl<T: VectorSpace> Interp<T> {
    #[inline]
    fn new(start: T, end: T, func: fn(f32) -> f32) -> Self {
        Self { start, end, func }
    }

    #[inline]
    fn sample(&self, t: f32) -> T {
        self.start.lerp(self.end, (self.func)(t))
    }
}

#[derive(Component, Copy, Clone)]
struct TrailParam {
    fade: Interp<f32>,
    side_fade: Interp<f32>,
    color: Interp<Vec3>,
    width: Interp<f32>,
}

impl TrailParam {
    fn soul() -> Self {
        Self {
            fade: Interp::new(0., 1., |t| t.powf(4.)),
            side_fade: Interp::new(0., 1., |t| t.powf(8.)),
            color: Interp::new(vec3(0., 1.5, 4.), vec3(2., 4.5, 15.), |t| t.powf(5.)),
            width: Interp::new(20., 20., |t| t),
        }
    }
}

#[derive(Component, AtlasEntries)]
struct TrailSprites {
    #[atlas]
    atlas: Handle<Atlas>,
    #[entry]
    body: PathBuf,
    #[entry]
    head: PathBuf,
}

fn update_trail(
    mut cursor_event: EventReader<CursorMoved>,
    time: Res<Time>,
    camera: Single<(&Camera, &GlobalTransform)>,
    mut trail: Single<&mut Trail, With<PrimaryTrail>>,
    mut cursor: Local<Vec2>,
    mut last_updated: Local<Duration>,
) -> Result {
    let (camera, camera_trns) = *camera;
    if let Some(e) = cursor_event.read().last() {
        *cursor = camera.viewport_to_world_2d(camera_trns, e.position)?;
    }

    *last_updated += time.delta();
    let div = last_updated.div_duration_f32(TRAIL_UPDATE_RATE);

    let count = div as u32;
    if count > 0 {
        *last_updated -= TRAIL_UPDATE_RATE * count;
        if let Some(remove) = (trail.points.len() + count as usize).checked_sub(trail.max_len.get()) {
            for _ in 0..remove {
                trail.points.pop_front();
            }
        }

        if let Some(&last_pos) = trail.points.back() {
            if (*cursor - last_pos).length_squared() < 10. {
                trail.still += 1;
                if trail.still >= 6 {
                    trail.shrinking = Some(trail.points.len())
                }
            } else {
                trail.still = 0;
                for i in 1..=count {
                    trail.points.push_back(last_pos.lerp(*cursor, i as f32 / div))
                }
            }
        } else {
            trail.points.push_back(*cursor)
        }

        if let Some(shrink) = trail.shrinking {
            trail.shrinking = shrink.checked_sub(count as usize);
            for _ in 0..count {
                trail.points.pop_front();
            }
        }
    }

    trail.head = *cursor;
    trail.tail = if trail.points.len() < trail.max_len.get() {
        trail.points.front().copied().unwrap_or(*cursor)
    } else {
        let from_pos = trail.points[0];
        let to_pos = trail.points.get(1).copied().unwrap_or(*cursor);

        from_pos.lerp(to_pos, div.fract())
    };

    Ok(())
}

#[derive(TypePath, Component)]
struct DrawTrail {
    points: Vec<Vec2>,
    max_len: usize,
    param: TrailParam,
    body: AtlasInfo,
    head: AtlasInfo,
}

impl Drawer for DrawTrail {
    type Vertex = Vert;

    type ExtractParam = ();
    type ExtractData = (Read<Trail>, Read<TrailParam>, Read<AtlasCaches>);
    type ExtractFilter = ();

    type DrawParam = ();

    #[inline]
    fn extract(
        drawer: DrawerExtract<Self>,
        _: &SystemParamItem<Self::ExtractParam>,
        (trail, &param, cache): QueryItem<Self::ExtractData>,
    ) {
        let this = match drawer {
            DrawerExtract::Borrowed(drawer) => {
                drawer.points.clear();
                drawer.max_len = trail.max_len.get();
                drawer.param = param;
                drawer
            }
            DrawerExtract::Spawn(spawn) => spawn.insert(Self {
                points: default(),
                max_len: default(),
                param,
                body: default(),
                head: default(),
            }),
        };

        let (front, back) = trail.points.as_slices();
        this.points.reserve_exact(front.len() + back.len() + 1);
        this.points.push(trail.tail);
        match (front.is_empty(), back.is_empty()) {
            (false, ..) => {
                this.points.extend_from_slice(&front[1..]); // Use `tail` instead of first point.
                this.points.extend_from_slice(back);
            }
            (true, false) => {
                this.points.extend_from_slice(&back[1..]); // Use `tail` instead of first point.
            }
            (true, true) => {}
        }

        this.points.push(trail.head);

        if let Some(&body) = cache.get(0) {
            this.body = body;
        }

        if let Some(&head) = cache.get(1) {
            this.head = head;
        }
    }

    fn draw(&self, _: &SystemParamItem<Self::DrawParam>, queuer: &impl VertexQueuer<Vertex = Self::Vertex>) {
        let Self {
            ref points,
            max_len,
            param:
                TrailParam {
                    fade,
                    side_fade,
                    color,
                    width,
                },
            body,
            head,
        } = *self;

        let mut total_len2 = 0.;
        for ab in points.windows(2) {
            let &[a, b] = ab else { break };
            total_len2 += (b - a).length_squared()
        }

        let Vec2 { x: u, y: v2 } = body.rect.min.as_vec2() / body.page_size.as_vec2();
        let Vec2 { x: u2, y: v } = body.rect.max.as_vec2() / body.page_size.as_vec2();
        let uc = (u + u2) * 0.5;

        let mut len2 = 0.;
        let mut prev_prog = 0.;
        let mut last_rot = (0., 1.);

        let max_prog = (points.len() - 2) as f32 / max_len as f32;
        for (i, ab) in points.windows(2).enumerate() {
            let &[a, b] = ab else { break };

            let rot = if (b - a).length_squared() <= 1. { last_rot } else { (-(b - a).to_angle()).sin_cos() };
            if i == 0 {
                last_rot = rot
            }

            len2 += (b - a).length_squared();
            let prog = (len2 / total_len2).sqrt() * max_prog;

            let pos0 = vec2(last_rot.0, last_rot.1) * width.sample(prev_prog);
            let pos1 = vec2(rot.0, rot.1) * width.sample(prog);
            let col0 = LinearRgba::from_f32_array(color.sample(prev_prog).extend(fade.sample(prev_prog)).to_array());
            let col1 = LinearRgba::from_f32_array(color.sample(prog).extend(fade.sample(prog)).to_array());
            let s_col0 = LinearRgba::from_f32_array(color.sample(prev_prog).extend(side_fade.sample(prev_prog)).to_array());
            let s_col1 = LinearRgba::from_f32_array(color.sample(prog).extend(side_fade.sample(prog)).to_array());
            let v0 = VectorSpace::lerp(v, v2, prev_prog / max_prog);
            let v1 = VectorSpace::lerp(v, v2, prog / max_prog);

            Shaper::new()
                .pos2d([a - pos0, a, b, b - pos1])
                .uv([[u, v0], [uc, v0], [uc, v1], [u, v1]].map(Vec2::from_array))
                .colors([s_col0, col0, col1, s_col1])
                .queue_rect(queuer, 0., body.page);

            Shaper::new()
                .pos2d([a + pos0, a, b, b + pos1])
                .uv([[u2, v0], [uc, v0], [uc, v1], [u2, v1]].map(Vec2::from_array))
                .colors([s_col0, col0, col1, s_col1])
                .queue_rect(queuer, 0., body.page);

            if i == points.len() - ab.len() {
                let w = width.sample(prog);
                let h = (head.rect.height() as f32 / head.rect.width() as f32) * w;
                let c = b + vec2(rot.1, -rot.0) * 2. * h;

                let Vec2 { x: u, y: v2 } = head.rect.min.as_vec2() / head.page_size.as_vec2();
                let Vec2 { x: u2, y: v } = head.rect.max.as_vec2() / head.page_size.as_vec2();
                let uc = (u + u2) * 0.5;

                Shaper::new()
                    .pos2d([b - pos1, b, c, c - pos1])
                    .uv([[u, v], [uc, v], [uc, v2], [u, v2]].map(Vec2::from_array))
                    .colors([s_col1, col1, col1, s_col1])
                    .queue_rect(queuer, 0., head.page);

                Shaper::new()
                    .pos2d([b + pos1, b, c, c + pos1])
                    .uv([[u2, v], [uc, v], [uc, v2], [u2, v2]].map(Vec2::from_array))
                    .colors([s_col1, col1, col1, s_col1])
                    .queue_rect(queuer, 0., head.page);
            } else {
                prev_prog = prog;
                last_rot = rot;
            }
        }
    }
}

fn main() -> AppExit {
    App::new()
        .add_plugins((DefaultPlugins, hephae! { atlas: TrailSprites, render: (Vert, DrawTrail) }))
        .add_systems(Startup, startup)
        .add_systems(PostUpdate, update_trail)
        .run()
}

fn startup(mut commands: Commands, server: Res<AssetServer>) {
    commands.spawn((
        Camera2d,
        Camera {
            clear_color: ClearColorConfig::Custom(Color::NONE),
            hdr: true,
            ..default()
        },
        Bloom {
            intensity: 0.5,
            low_frequency_boost_curvature: 0.,
            ..Bloom::NATURAL
        },
        Tonemapping::TonyMcMapface,
    ));

    commands.spawn((
        PrimaryTrail,
        Trail::new(NonZeroUsize::new(TRAIL_LENGTH).unwrap()),
        TrailParam::soul(),
        TrailSprites {
            atlas: server.load("sprites/sprites.atlas.ron"),
            body: "trails/soul".into(),
            head: "trails/soul-cap".into(),
        },
    ));
}
