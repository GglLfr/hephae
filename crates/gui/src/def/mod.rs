use bevy_app::prelude::*;

mod layout;
mod root;

pub use layout::*;
pub use root::*;

use crate::{gui::GuiLayoutPlugin, space::GuiRootPlugin};

#[derive(Copy, Clone, Default)]
pub struct DefaultUiPlugin;
impl Plugin for DefaultUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((GuiLayoutPlugin::<Cont>::new(), GuiRootPlugin::<FromCamera2d>::new()));
    }
}
