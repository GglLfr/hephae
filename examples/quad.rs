use bevy::{
    core_pipeline::{bloom::Bloom, core_2d::Transparent2d},
    ecs::{
        query::QueryItem,
        system::{SystemParamItem, lifetimeless::SRes},
    },
    math::FloatOrd,
    prelude::*,
    render::{
        render_phase::{DrawFunctionId, PhaseItemExtraIndex},
        render_resource::{CachedRenderPipelineId, RenderPipelineDescriptor},
        sync_world::MainEntity,
    },
};
use bytemuck::{Pod, Zeroable};
use hephae::prelude::*;

#[derive(VertexLayout, Copy, Clone, Pod, Zeroable)]
#[repr(C)]
struct Vert {
    pos: [f32; 2],
    color: LinearRgba,
}

impl Vert {
    #[inline]
    const fn new(x: f32, y: f32, red: f32, green: f32, blue: f32, alpha: f32) -> Self {
        Self {
            pos: [x, y],
            color: LinearRgba { red, green, blue, alpha },
        }
    }
}

impl Vertex for Vert {
    type PipelineParam = ();
    type PipelineProp = ();
    type PipelineKey = ();

    type BatchParam = ();
    type BatchProp = ();

    type Item = Transparent2d;
    type RenderCommand = ();

    const SHADER: &'static str = "quad.wgsl";

    #[inline]
    fn init_pipeline(_: SystemParamItem<Self::PipelineParam>) -> Self::PipelineProp {}

    #[inline]
    fn specialize_pipeline(_: Self::PipelineKey, _: &Self::PipelineProp, _: &mut RenderPipelineDescriptor) {}

    #[inline]
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
    fn create_batch(_: &mut SystemParamItem<Self::BatchParam>, _: Self::PipelineKey) -> Self::BatchProp {}
}

#[derive(TypePath, Component, Copy, Clone)]
struct Draw;
impl Drawer for Draw {
    type Vertex = Vert;

    type ExtractParam = ();
    type ExtractData = ();
    type ExtractFilter = ();

    type DrawParam = SRes<Time>;

    #[inline]
    fn extract(mut drawer: DrawerExtract<Self>, _: &SystemParamItem<Self::ExtractParam>, _: QueryItem<Self::ExtractData>) {
        *drawer.get_mut(|| Self) = Self;
    }

    #[inline]
    fn draw(&mut self, time: &SystemParamItem<Self::DrawParam>, queuer: &impl VertexQueuer<Vertex = Self::Vertex>) {
        let (sin, cos) = (time.elapsed_secs() * 3.).sin_cos();
        let base = queuer.data([
            Vert::new(100. + cos * 25., 100. + sin * 25., 2., 0., 0., 1.),
            Vert::new(-100. - cos * 25., 100. + sin * 25., 0., 3., 0., 1.),
            Vert::new(-100. - cos * 25., -100. - sin * 25., 0., 0., 4., 1.),
            Vert::new(100. + cos * 25., -100. - sin * 25., 4., 3., 2., 1.),
        ]);

        queuer.request(0., (), [base, base + 1, base + 2, base + 2, base + 3, base]);
    }
}

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, hephae::render::<Vert, Draw>()))
        .add_systems(Startup, startup)
        .run();
}

fn startup(mut commands: Commands) {
    commands.spawn((Camera2d, Camera { hdr: true, ..default() }, Bloom::NATURAL));
    commands.spawn((Transform::IDENTITY, HasDrawer::<Draw>::new()));
}
