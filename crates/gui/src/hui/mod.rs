use bevy_app::prelude::*;

mod layout;
mod root;

pub use layout::*;
pub use root::*;

use crate::{gui::GuiLayoutPlugin, root::GuiRootPlugin};

#[derive(Copy, Clone, Default)]
pub struct HuiPlugin;
impl Plugin for HuiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((GuiLayoutPlugin::<ContLayout>::new(), GuiRootPlugin::<FromCamera2d>::new()));
    }
}
