//! Provides built-in GUI modules for convenience.

use bevy_app::prelude::*;

mod layout;
mod root;

pub use layout::*;
pub use root::*;

use crate::gui::{GuiLayoutPlugin, GuiRootPlugin};

/// Registers the built-in GUI modules to the application.
#[derive(Copy, Clone, Default)]
pub struct DefaultUiPlugin;
impl Plugin for DefaultUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((GuiLayoutPlugin::<Cont>::new(), GuiRootPlugin::<FromCamera2d>::new()));
    }
}
