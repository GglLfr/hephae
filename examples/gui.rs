use bevy::prelude::*;
use hephae::prelude::*;

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, HephaeGuiPlugin))
        .add_systems(Startup, startup)
        .run();
}

fn startup(mut commands: Commands) {
    commands
        .spawn((Camera2d, hui::FromCamera2d, hui::Cont::Horizontal))
        .with_children(|ui| {
            ui.spawn((
                hui::Cont::Horizontal,
                hui::Size(hui::ValSize::new(hui::Frac(0.5), hui::Frac(1.))),
            ))
            .with_children(|ui| {
                ui.spawn((hui::Cont::Horizontal, hui::Size(hui::ValSize::all(hui::Auto))))
                    .with_children(|ui| {
                        for _ in 0..3 {
                            ui.spawn((hui::Cont::Horizontal, hui::Size(hui::ValSize::all(hui::Px(40.)))));
                        }
                    });
            });
        });
}
