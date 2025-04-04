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
    pos: [f32; 2],
    color: LinearRgba,
}

impl Vert {
    #[inline]
    const fn new(pos: Vec2, color: LinearRgba) -> Self {
        Self {
            pos: pos.to_array(),
            color,
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
    fn create_batch(_: &mut SystemParamItem<Self::BatchParam>, _: Self::PipelineKey) -> Self::BatchProp {}
}

#[derive(TypePath, Component, Copy, Clone, Default)]
struct Draw {
    color: LinearRgba,
    transform: Affine3A,
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
        drawer.transform = trns.affine();
        drawer.size = ui.size;
    }

    #[inline]
    fn draw(&mut self, _: &SystemParamItem<Self::DrawParam>, queuer: &impl VertexQueuer<Vertex = Self::Vertex>) {
        let Self { color, transform, size } = *self;
        let base = queuer.data(
            [
                transform.translation.truncate(),
                transform.transform_point(Vec3::new(size.x, 0., 0.)).truncate(),
                transform.transform_point(Vec3::new(size.x, size.y, 0.)).truncate(),
                transform.transform_point(Vec3::new(0., size.y, 0.)).truncate(),
            ]
            .map(|pos| Vert::new(pos, color)),
        );

        queuer.request(0., (), [base, base + 1, base + 2, base + 2, base + 3, base]);
    }
}

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, hephae! { render: (Vert, Draw), ui }))
        .add_systems(Startup, startup)
        .add_systems(Update, rotate)
        .run();
}

#[derive(Component, Copy, Clone)]
struct Rotate;

#[derive(Component, Copy, Clone)]
struct Color(LinearRgba);

fn startup(mut commands: Commands) {
    commands.spawn(Camera2dRoot::default()).with_children(|ui| {
        ui.spawn((
            Ui {
                size: UiSize::new(Vw(1.), Vh(0.8)),
                padding: UiBorder::all(Abs(25.)),
                ..default()
            },
            Color(LinearRgba::RED),
            HasDrawer::<Draw>::new(),
        ))
        .with_children(|ui| {
            ui.spawn((
                Rotate,
                Ui {
                    max_size: UiSize::rel(0.5, 1.),
                    flex_grow: 1.,
                    flex_direction: FlexDirection::Column,
                    ..default()
                },
                Color(LinearRgba::GREEN),
                HasDrawer::<Draw>::new(),
            ))
            .with_children(|ui| {
                ui.spawn((
                    Rotate,
                    Ui {
                        margin: UiBorder::all(Abs(10.)),
                        ..default()
                    },
                    Color(LinearRgba::BLUE),
                    HasDrawer::<Draw>::new(),
                ))
                .with_children(|ui| {
                    for _ in 0..3 {
                        ui.spawn((
                            Ui {
                                size: UiSize::all(Abs(40.)),
                                margin: UiBorder::all(Abs(10.)),
                                ..default()
                            },
                            Color(LinearRgba::WHITE),
                            HasDrawer::<Draw>::new(),
                        ));
                    }
                });
            });
        });
    });
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
