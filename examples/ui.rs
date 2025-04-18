use bevy::{
    core_pipeline::core_2d::Transparent2d,
    ecs::{
        query::QueryItem,
        system::{SystemParamItem, lifetimeless::Read},
    },
    math::Affine3A,
    prelude::*,
};
use hephae::prelude::*;

#[derive(VertexLayout, Copy, Clone, Pod, Zeroable)]
#[bytemuck(crate = "hephae::render::bytemuck")]
#[repr(C)]
struct Vert {
    #[attrib(Pos3d)]
    pos: Vec3,
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

#[derive(TypePath, Component, Copy, Clone, Default)]
struct Draw {
    color: LinearRgba,
    trns: Affine3A,
    size: Vec2,
}

impl Drawer for Draw {
    type Vertex = Vert;

    type ExtractParam = ();
    type ExtractData = (Read<GlobalTransform>, Read<ComputedUi>, Read<Color>);
    type ExtractFilter = ();

    type DrawParam = ();

    #[inline]
    fn extract(
        mut drawer: DrawerExtract<Self>,
        _: &SystemParamItem<Self::ExtractParam>,
        (trns, ui, color): QueryItem<Self::ExtractData>,
    ) {
        let drawer = drawer.get_or_default();
        drawer.color = color.0;
        drawer.trns = trns.affine();
        drawer.size = ui.size;
    }

    #[inline]
    fn draw(&self, _: &SystemParamItem<Self::DrawParam>, queuer: &impl VertexQueuer<Vertex = Self::Vertex>) {
        let Self { color, trns, size } = *self;
        Shaper::new()
            .pos3d([
                Vec3::from(trns.translation),
                trns.transform_point(Vec3::new(size.x, 0., 0.)),
                trns.transform_point(Vec3::new(size.x, size.y, 0.)),
                trns.transform_point(Vec3::new(0., size.y, 0.)),
            ])
            .color(color)
            .queue_rect(queuer, trns.translation.z, ())
    }
}

fn main() -> AppExit {
    App::new()
        .add_plugins((DefaultPlugins, hephae! { render: (Vert, Draw), ui }))
        .add_systems(Startup, startup)
        .add_systems(Update, rotate)
        .run()
}

#[derive(Component, Copy, Clone)]
struct Rotate;

#[derive(Component, Copy, Clone)]
struct Color(LinearRgba);

fn startup(mut commands: Commands) {
    let leaf = || {
        (
            Ui {
                size: UiSize::all(Abs(40.)),
                margin: UiBorder::all(Abs(10.)),
                ..default()
            },
            Color(LinearRgba::WHITE),
            DrawBy::<Draw>::new(),
        )
    };

    commands.spawn((Camera2dRoot::default(), children![(
        Ui {
            size: UiSize::new(Vw(1.), Vh(0.8)),
            padding: UiBorder::all(Abs(25.)),
            ..default()
        },
        Color(LinearRgba::RED),
        DrawBy::<Draw>::new(),
        children![(
            Rotate,
            Ui {
                max_size: UiSize::rel(0.5, 1.),
                flex_grow: 1.,
                flex_direction: FlexDirection::Column,
                ..default()
            },
            Color(LinearRgba::GREEN),
            DrawBy::<Draw>::new(),
            children![(
                Rotate,
                Ui {
                    margin: UiBorder::all(Abs(10.)),
                    ..default()
                },
                Color(LinearRgba::BLUE),
                DrawBy::<Draw>::new(),
                children![leaf(), leaf(), leaf(), leaf()]
            )]
        )]
    )]));
}

fn rotate(time: Res<Time>, mut rotate: Query<&mut Ui, With<Rotate>>, mut timer: Local<f64>) {
    *timer += time.delta_secs_f64();
    if *timer >= 1. {
        *timer -= 1.;
        for mut cont in &mut rotate {
            cont.flex_direction = match cont.flex_direction {
                FlexDirection::Row => FlexDirection::RowReverse,
                FlexDirection::RowReverse => FlexDirection::Column,
                FlexDirection::Column => FlexDirection::ColumnReverse,
                FlexDirection::ColumnReverse => FlexDirection::Row,
            }
        }
    }
}
