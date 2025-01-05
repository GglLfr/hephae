//! Provides built-in GUI modules for convenience.

use bevy_app::{prelude::*, PluginGroupBuilder};

mod layout;
mod root;
#[cfg(feature = "text")]
mod text;

pub use layout::*;
pub use root::*;
#[cfg(feature = "text")]
pub use text::*;

use crate::gui::{GuiLayoutPlugin, GuiRootPlugin};

/// Registers the built-in GUI modules to the application.
#[derive(Copy, Clone, Default)]
pub struct DefaultUiPlugin;
impl PluginGroup for DefaultUiPlugin {
    fn build(self) -> PluginGroupBuilder {
        let mut group = PluginGroupBuilder::start::<Self>();
        group = group
            .add(GuiLayoutPlugin::<UiCont>::new())
            .add(GuiRootPlugin::<FromCamera2d>::new());

        #[cfg(feature = "text")]
        {
            group = group.add(GuiLayoutPlugin::<UiText>::new());
        }

        group
    }
}
