use bevy::prelude::*;
use hephae::prelude::*;

fn main() {
    App::new().add_plugins((DefaultPlugins, HephaeGuiPlugin)).run();
}
