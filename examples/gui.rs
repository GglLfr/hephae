use bevy::prelude::*;
use hephae::prelude::*;
use hephae_gui::{gui::Gui, HephaeGuiSystems};

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, HephaeGuiPlugin))
        .add_systems(Startup, startup)
        .add_systems(PostUpdate, print_ui.run_if(run_once).after(HephaeGuiSystems::PropagateLayout))
        .run();
}

fn startup(mut commands: Commands) {
    commands
        .spawn((Camera2d, FromCamera2d, Cont::Horizontal))
        .with_children(|ui| {
            ui.spawn((Cont::Horizontal, UiSize::new(Rel(0.5), Rel(1.))))
                .with_children(|ui| {
                    ui.spawn((Cont::Horizontal, UiSize::all(Auto), Padding::all(10.)))
                        .with_children(|ui| {
                            for _ in 0..3 {
                                ui.spawn((Cont::Horizontal, UiSize::all(Abs(40.)), Margin::all(10.)));
                            }
                        });
                });
        });
}

fn print_ui(query: Query<(&Gui, Option<&Children>)>, root: Query<Entity, (With<Gui>, Without<Parent>)>) {
    fn print(indent: &mut String, e: Entity, query: &Query<(&Gui, Option<&Children>)>) {
        let Ok((&gui, children)) = query.get(e) else { return };
        println!(
            "{indent}{e}: [{}, {}, {}, {}]",
            gui.bottom_left, gui.bottom_right, gui.top_right, gui.top_left
        );

        if let Some(children) = children {
            indent.push_str("|   ");
            for &child in children {
                print(indent, child, query);
            }
            indent.truncate(indent.len() - 4);
        }
    }

    for e in &root {
        print(&mut String::new(), e, &query);
    }
}
