use std::mem::offset_of;

use bevy::{
    core_pipeline::core_2d::Transparent2d,
    ecs::{
        query::QueryItem,
        system::{lifetimeless::Read, SystemParamItem},
    },
    math::{vec2, FloatOrd},
    prelude::*,
    render::{
        render_phase::{DrawFunctionId, PhaseItemExtraIndex},
        render_resource::{BufferAddress, CachedRenderPipelineId, RenderPipelineDescriptor, VertexAttribute, VertexFormat},
        sync_world::MainEntity,
    },
};
use bytemuck::{Pod, Zeroable};
use hephae::prelude::*;
use hephae_gui::gui::{Gui, GuiDepth};

#[derive(Copy, Clone, Pod, Zeroable)]
#[repr(C)]
struct Vert {
    pos: [f32; 2],
    depth: f32,
}

impl Vert {
    #[inline]
    const fn new(Vec2 { x, y }: Vec2, depth: f32) -> Self {
        Self { pos: [x, y], depth }
    }
}

impl Vertex for Vert {
    type PipelineParam = ();
    type PipelineProp = ();
    type PipelineKey = ();

    type Command = DrawGui;

    type BatchParam = ();
    type BatchProp = ();

    type Item = Transparent2d;
    type RenderCommand = ();

    const SHADER: &'static str = "gui.wgsl";
    const LAYOUT: &'static [VertexAttribute] = &[
        VertexAttribute {
            format: VertexFormat::Float32x2,
            offset: offset_of!(Self, pos) as BufferAddress,
            shader_location: 0,
        },
        VertexAttribute {
            format: VertexFormat::Float32,
            offset: offset_of!(Self, depth) as BufferAddress,
            shader_location: 1,
        },
    ];

    #[inline]
    fn init_pipeline(_: SystemParamItem<Self::PipelineParam>) -> Self::PipelineProp {}

    #[inline]
    fn specialize_pipeline(_: Self::PipelineKey, _: &Self::PipelineProp, _: &mut RenderPipelineDescriptor) {}

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
struct Draw(Gui, GuiDepth);
impl Drawer for Draw {
    type Vertex = Vert;

    type ExtractParam = ();
    type ExtractData = (Read<Gui>, Read<GuiDepth>);
    type ExtractFilter = ();

    type DrawParam = ();

    #[inline]
    fn extract(_: &SystemParamItem<Self::ExtractParam>, (&gui, &gui_depth): QueryItem<Self::ExtractData>) -> Option<Self> {
        Some(Self(gui, gui_depth))
    }

    #[inline]
    fn enqueue(
        &self,
        _: &SystemParamItem<Self::DrawParam>,
        queuer: &mut impl Extend<(f32, <Self::Vertex as Vertex>::PipelineKey, <Self::Vertex as Vertex>::Command)>,
    ) {
        let Gui {
            bottom_left,
            bottom_right,
            top_right,
            top_left,
            ..
        } = self.0;

        queuer.extend([(
            self.1.depth as f32,
            (),
            DrawGui([bottom_left, bottom_right, top_right, top_left], self.1),
        )]);
    }
}

#[derive(TypePath, Component, Clone)]
struct DrawText(Gui, GuiDepth, Vec<TextGlyph>);
impl Drawer for DrawText {
    type Vertex = Vert;

    type ExtractParam = ();
    type ExtractData = (Read<Gui>, Read<GuiDepth>, Read<TextGlyphs>);
    type ExtractFilter = ();

    type DrawParam = ();

    #[inline]
    fn extract(
        _: &SystemParamItem<Self::ExtractParam>,
        (&gui, &gui_depth, glyphs): QueryItem<Self::ExtractData>,
    ) -> Option<Self> {
        Some(Self(gui, gui_depth, glyphs.glyphs.clone()))
    }

    #[inline]
    fn enqueue(
        &self,
        _: &SystemParamItem<Self::DrawParam>,
        queuer: &mut impl Extend<(f32, <Self::Vertex as Vertex>::PipelineKey, <Self::Vertex as Vertex>::Command)>,
    ) {
        let Gui {
            bottom_left,
            bottom_right,
            top_right,
            top_left,
            ..
        } = self.0;

        let gui = self.0;
        let base = bottom_left;
        queuer.extend(
            [(
                self.1.depth as f32,
                (),
                DrawGui([bottom_left, bottom_right, top_right, top_left], self.1),
            )]
            .into_iter()
            .chain(self.2.iter().map(|&glyph| {
                (
                    (self.1.depth + 1) as f32,
                    (),
                    DrawGui(
                        [
                            base + gui.project(glyph.origin),
                            base + gui.project(glyph.origin + vec2(glyph.size.x, 0.)),
                            base + gui.project(glyph.origin + glyph.size),
                            base + gui.project(glyph.origin + vec2(0., glyph.size.y)),
                        ],
                        GuiDepth {
                            depth: self.1.depth + 1,
                            total_depth: self.1.total_depth,
                        },
                    ),
                )
            })),
        );
    }
}

#[derive(Copy, Clone)]
struct DrawGui([Vec3; 4], GuiDepth);
impl VertexCommand for DrawGui {
    type Vertex = Vert;

    #[inline]
    fn draw(&self, queuer: &mut impl VertexQueuer<Vertex = Self::Vertex>) {
        let &Self([bottom_left, bottom_right, top_right, top_left], GuiDepth { depth, total_depth }) = self;

        let depth = (depth as f32) / ((total_depth + 1) as f32);
        queuer.vertices([
            Vert::new(bottom_left.truncate(), depth),
            Vert::new(bottom_right.truncate(), depth),
            Vert::new(top_right.truncate(), depth),
            Vert::new(top_left.truncate(), depth),
        ]);

        queuer.indices([0, 1, 2, 2, 3, 0]);
    }
}

#[derive(Component, Copy, Clone)]
struct Rotate;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            HephaeRenderPlugin::<Vert>::new(),
            DrawerPlugin::<Draw>::new(),
            DrawerPlugin::<DrawText>::new(),
            HephaeTextPlugin,
            HephaeGuiPlugin,
        ))
        .add_systems(Startup, startup)
        .add_systems(Update, rotate)
        .run();
}

fn startup(mut commands: Commands, server: Res<AssetServer>) {
    commands
        .spawn((Camera2d, FromCamera2d, UiCont::Horizontal, HasDrawer::<Draw>::new()))
        .with_children(|ui| {
            ui.spawn((
                Rotate,
                UiCont::Horizontal,
                UiSize::new(Rel(0.5), Rel(1.)),
                Margin::all(25.),
                Padding::xy(0., 10.),
                HasDrawer::<Draw>::new(),
            ))
            .with_children(|ui| {
                ui.spawn((
                    Rotate,
                    UiCont::Horizontal,
                    UiSize::all(Auto),
                    Margin::all(10.),
                    Shrink(Vec2::ONE),
                    HasDrawer::<Draw>::new(),
                ))
                .with_children(|ui| {
                    for _ in 0..3 {
                        ui.spawn((
                            UiCont::Horizontal,
                            UiSize::all(Abs(40.)),
                            Margin::all(10.),
                            Shrink(Vec2::ONE),
                            HasDrawer::<Draw>::new(),
                        ));
                    }
                });

                ui.spawn((
                    UiText,
                    UiSize::all(Auto),
                    Margin::all(10.),
                    Text {
                        text: "Hi, Hephae GUI!".into(),
                        align: TextAlign::Center,
                        ..default()
                    },
                    TextFont {
                        font: server.load("fonts/roboto.ttf"),
                        font_size: 24.,
                        ..default()
                    },
                    Expand(vec2(1., 0.)),
                    Shrink(Vec2::ONE),
                    HasDrawer::<DrawText>::new(),
                ));
            });
        });
}

fn rotate(
    time: Res<Time>,
    mut camera: Query<&mut OrthographicProjection>,
    mut rotate: Query<&mut UiCont, With<Rotate>>,
    mut timer: Local<f64>,
) {
    *timer += time.delta_secs_f64();
    if *timer >= 0.5 {
        *timer -= 0.5;
        for mut cont in &mut rotate {
            *cont = match *cont {
                UiCont::Horizontal => UiCont::HorizontalReverse,
                UiCont::HorizontalReverse => UiCont::Vertical,
                UiCont::Vertical => UiCont::VerticalReverse,
                UiCont::VerticalReverse => UiCont::Horizontal,
            };
        }
    }

    let Ok(mut proj) = camera.get_single_mut() else { return };
    proj.scale = 1.5 + ((time.elapsed_secs() * 4.).sin() + 1.0) * 0.25;
}
