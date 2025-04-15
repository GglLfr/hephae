use bevy::{
    core_pipeline::{bloom::Bloom, core_2d::Transparent2d, tonemapping::Tonemapping},
    ecs::{
        query::QueryItem,
        system::{SystemParamItem, lifetimeless::SRes},
    },
    prelude::*,
};
use hephae::prelude::*;

#[derive(VertexLayout, Copy, Clone, Pod, Zeroable)]
#[bytemuck(crate = "hephae::render::bytemuck")]
#[repr(C)]
struct Vert {
    #[attrib(Pos2d)]
    pos: Vec2,
    #[attrib(Color)]
    color: LinearRgba,
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
        *drawer.get_mut(|| Self) = Self
    }

    #[inline]
    fn draw(&mut self, time: &SystemParamItem<Self::DrawParam>, queuer: &impl VertexQueuer<Vertex = Self::Vertex>) {
        let (sin, cos) = (time.elapsed_secs() * 3.).sin_cos();
        Shaper::new()
            .pos2d(
                [
                    [100. + cos * 25., 100. + sin * 25.],
                    [-100. - cos * 25., 100. + sin * 25.],
                    [-100. - cos * 25., -100. - sin * 25.],
                    [100. + cos * 25., -100. - sin * 25.],
                ]
                .map(Vec2::from_array),
            )
            .colors(
                [[2., 0., 0., 1.], [0., 3., 0., 1.], [0., 0., 4., 1.], [2., 5., 50., 1.]].map(LinearRgba::from_f32_array),
            )
            .queue_rect(queuer, 0., ())
    }
}

fn main() -> AppExit {
    App::new()
        .add_plugins((DefaultPlugins, hephae! { render: (Vert, Draw) }))
        .add_systems(Startup, startup)
        .run()
}

fn startup(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        Camera {
            hdr: true,
            clear_color: ClearColorConfig::Custom(Color::BLACK),
            ..default()
        },
        Bloom {
            intensity: 0.5,
            low_frequency_boost_curvature: 0.,
            ..Bloom::NATURAL
        },
        Tonemapping::TonyMcMapface,
    ));
    commands.spawn(HasDrawer::<Draw>::new());
}
