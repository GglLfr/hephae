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
        .spawn((Camera2d, FromCamera2d, ContLayout::Horizontal))
        .with_children(|ui| {
            ui.spawn((ContLayout::Horizontal, HuiSize(HuiVal2::new(Frac(0.5), Frac(1.)))))
                .with_children(|ui| {
                    ui.spawn((
                        ContLayout::Horizontal,
                        HuiSize(HuiVal2::all(Auto)),
                        HuiPadding(HuiRect::all(10.)),
                    ))
                    .with_children(|ui| {
                        for _ in 0..3 {
                            ui.spawn((
                                ContLayout::Horizontal,
                                HuiSize(HuiVal2::all(Px(40.))),
                                HuiMargin(HuiRect::all(10.)),
                            ));
                        }
                    });
                });
        });
}
