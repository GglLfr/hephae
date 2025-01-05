//! Provides built-in GUI modules for convenience.

use bevy_app::prelude::*;
use bevy_ecs::prelude::*;

mod layout;
mod root;
#[cfg(feature = "text")]
mod text;

pub use layout::*;
pub use root::*;
#[cfg(feature = "text")]
pub use text::*;

use crate::{
    gui::{GuiLayoutPlugin, GuiRootPlugin},
    HephaeGuiSystems,
};

/// Registers the built-in GUI modules to the application.
#[derive(Copy, Clone, Default)]
pub struct DefaultUiPlugin;
impl Plugin for DefaultUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((GuiLayoutPlugin::<UiCont>::new(), GuiRootPlugin::<FromCamera2d>::new()));

        #[cfg(feature = "text")]
        {
            app.add_plugins(GuiLayoutPlugin::<UiText>::new())
                .add_systems(PostUpdate, update_text_widget.after(HephaeGuiSystems::CalculateCorners));
        }
    }
}
