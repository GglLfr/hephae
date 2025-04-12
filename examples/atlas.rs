use bevy::{
    core_pipeline::{bloom::Bloom, core_2d::Transparent2d},
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
};
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
    pos: Vec2,
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
            pos: translation.truncate(),
            scl: scale.truncate(),
            page,
            page_size,
            rect,
        };
    }

    #[inline]
    fn draw(&mut self, _: &SystemParamItem<Self::DrawParam>, queuer: &impl VertexQueuer<Vertex = Self::Vertex>) {
        Shaper::new()
            .rect(self.pos, self.rect.size().as_vec2() * self.scl)
            .uv_rect(self.rect, self.page_size)
            .queue_rect(queuer, 0., self.page)
    }
}

fn main() -> AppExit {
    App::new()
        .add_plugins((DefaultPlugins.set(ImagePlugin::default_nearest()), hephae! {
            atlas,
            render: (Vert, DrawSprite),
        }))
        .add_systems(Startup, startup)
        .add_systems(Update, check)
        .run()
}

fn check(
    server: Res<AssetServer>,
    images: Res<Assets<Image>>,
    mut handle: Local<Option<Handle<Image>>>,
    mut done: Local<bool>,
) {
    if *done {
        return
    }

    let handle = handle.get_or_insert_with(|| server.load("sprites/sprites.atlas.ron#page-0"));
    if let Some(image) = images.get(handle) {
        image.clone().try_into_dynamic().unwrap().save("output.png").unwrap();
        *done = true
    }
}

fn startup(mut commands: Commands, server: Res<AssetServer>) {
    commands.spawn((Camera2d, Camera { hdr: true, ..default() }, Bloom::NATURAL));

    for translation in [
        Vec3::new(-200., -200., 0.),
        Vec3::new(200., -200., 0.),
        Vec3::new(200., 200., 0.),
        Vec3::new(-200., 200., 0.),
    ] {
        commands.spawn((
            Transform {
                translation,
                scale: Vec3::splat(10.),
                ..default()
            },
            AtlasEntry::new(server.load("sprites/sprites.atlas.ron"), "cix"),
            HasDrawer::<DrawSprite>::new(),
        ));
    }
}
