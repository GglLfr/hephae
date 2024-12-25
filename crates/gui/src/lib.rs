#![doc = include_str!("../README.md")]
#![cfg_attr(doc, deny(missing_docs))]

pub mod def;
pub mod gui;
pub mod layout;
pub mod space;

use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use bevy_render::camera::CameraUpdateSystem;

use crate::{
    def::DefaultUiPlugin,
    gui::GuiLayouts,
    layout::propagate_layout,
    space::{calculate_corners, validate_root, GuiRoots},
};

pub mod prelude {
    pub use crate::{def::*, HephaeGuiPlugin};
}

#[derive(SystemSet, Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum HephaeGuiSystems {
    CalculatePreferredSize,
    CalculateRoot,
    ValidateRoot,
    PropagateLayout,
    CalculateCorners,
}

#[derive(Copy, Clone, Default)]
pub struct HephaeGuiPlugin;
impl Plugin for HephaeGuiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GuiLayouts>()
            .init_resource::<GuiRoots>()
            .configure_sets(
                PostUpdate,
                (
                    (
                        HephaeGuiSystems::CalculatePreferredSize.ambiguous_with(HephaeGuiSystems::CalculatePreferredSize),
                        HephaeGuiSystems::CalculateRoot,
                        HephaeGuiSystems::ValidateRoot,
                    )
                        .before(HephaeGuiSystems::PropagateLayout)
                        .after(CameraUpdateSystem),
                    (HephaeGuiSystems::PropagateLayout, HephaeGuiSystems::CalculateCorners).chain(),
                ),
            )
            .add_systems(
                PostUpdate,
                (
                    validate_root.in_set(HephaeGuiSystems::ValidateRoot),
                    propagate_layout.in_set(HephaeGuiSystems::PropagateLayout),
                    calculate_corners.in_set(HephaeGuiSystems::CalculateCorners),
                ),
            )
            .add_plugins(DefaultUiPlugin);
    }
}
