#![doc = include_str!("../README.md")]
#![cfg_attr(doc, deny(missing_docs))]

pub mod gui;
pub mod layout;

use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use bevy_transform::TransformSystem;

use crate::{gui::GuiLayouts, layout::calculate_preferred_layout_size};

pub mod prelude {
    pub use crate::HephaeGuiPlugin;
}

#[derive(SystemSet, Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum HephaeGuiSystems {
    CalculatePreferredSize,
    CalculatePreferredLayoutSize,
}

#[derive(Copy, Clone, Default)]
pub struct HephaeGuiPlugin;
impl Plugin for HephaeGuiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GuiLayouts>()
            .configure_sets(
                PostUpdate,
                (
                    HephaeGuiSystems::CalculatePreferredSize,
                    HephaeGuiSystems::CalculatePreferredLayoutSize,
                )
                    .chain()
                    .before(TransformSystem::TransformPropagate),
            )
            .add_systems(
                PostUpdate,
                calculate_preferred_layout_size.in_set(HephaeGuiSystems::CalculatePreferredLayoutSize),
            );
    }
}
