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
struct SpriteVertex {
    pos: [f32; 2],
    uv: [f32; 2],
}

impl SpriteVertex {
    #[inline]
    pub const fn new(x: f32, y: f32, u: f32, v: f32) -> Self {
        Self { pos: [x, y], uv: [u, v] }
    }
}

impl Vertex for SpriteVertex {
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
    type Param = (SRes<ImageBindGroups>, SRes<ViewBatches<SpriteVertex>>);
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
    rect: URect,
}

impl Drawer for DrawSprite {
    type Vertex = SpriteVertex;

    type ExtractParam = SRes<Assets<Atlas>>;
    type ExtractData = (Read<GlobalTransform>, Read<AtlasEntry>, Read<AtlasIndex>);
    type ExtractFilter = ();

    type DrawParam = SRes<RenderAssets<GpuImage>>;

    #[inline]
    fn extract(
        mut drawer: DrawerExtract<Self>,
        atlases: &SystemParamItem<Self::ExtractParam>,
        (&trns, atlas, index): QueryItem<Self::ExtractData>,
    ) {
        (|| -> Option<()> {
            let atlas = atlases.get(&atlas.atlas)?;
            let (page_index, rect_index) = index.indices()?;

            let (page, (rect, ..)) = atlas
                .pages
                .get(page_index)
                .and_then(|page| Some((page.image.id(), *page.sprites.get(rect_index)?)))?;

            let (scale, .., translation) = trns.to_scale_rotation_translation();
            *drawer.get_or_default() = DrawSprite {
                pos: translation.truncate(),
                scl: scale.truncate(),
                page,
                rect,
            };

            None
        })();
    }

    #[inline]
    fn draw(&mut self, images: &SystemParamItem<Self::DrawParam>, queuer: &impl VertexQueuer<Vertex = Self::Vertex>) {
        let Some(page) = images.get(self.page) else {
            return;
        };

        let Vec2 { x, y } = self.pos;
        let Vec2 { x: hw, y: hh } = (self.rect.max - self.rect.min).as_vec2() / 2. * self.scl;
        let Vec2 { x: u, y: v2 } = self.rect.min.as_vec2() / vec2(page.size.width as f32, page.size.height as f32);
        let Vec2 { x: u2, y: v } = self.rect.max.as_vec2() / vec2(page.size.width as f32, page.size.height as f32);

        let base = queuer.data([
            SpriteVertex::new(x - hw, y - hh, u, v),
            SpriteVertex::new(x + hw, y - hh, u2, v),
            SpriteVertex::new(x + hw, y + hh, u2, v2),
            SpriteVertex::new(x - hw, y + hh, u, v2),
        ]);

        queuer.request(0., self.page, [base, base + 1, base + 2, base + 2, base + 3, base]);
    }
}

fn main() {
    App::new()
        .add_plugins((DefaultPlugins.set(ImagePlugin::default_nearest()), hephae! {
            atlas,
            render: (SpriteVertex, DrawSprite),
        }))
        .add_systems(Startup, startup)
        .run();
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
            AtlasEntry {
                atlas: server.load::<Atlas>("sprites/sprites.atlas.ron"),
                key: "cix".into(),
            },
            HasDrawer::<DrawSprite>::new(),
        ));
    }
}
