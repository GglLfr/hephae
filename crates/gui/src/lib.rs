#![doc = include_str!("../README.md")]
#![cfg_attr(doc, deny(missing_docs))]

pub mod def;
pub mod gui;
pub(crate) mod layout;

use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use bevy_render::camera::CameraUpdateSystem;
#[cfg(feature = "text")]
use hephae_text::HephaeTextSystems;

use crate::{
    def::DefaultUiPlugin,
    gui::{GuiLayouts, GuiRoots},
    layout::{calculate_corners, propagate_layout, validate_root},
};

/// Common imports for [`hephae_gui`](crate).
pub mod prelude {
    pub use crate::{
        def::*,
        gui::{GuiLayout, GuiLayoutPlugin, GuiRoot, GuiRootPlugin},
        HephaeGuiPlugin,
    };
}

/// Labels assigned to Hephae systems that are added to [`PostUpdate`], responsible over all GUI
/// layout calculations.
#[derive(SystemSet, Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum HephaeGuiSystems {
    /// Calculates [`GuiRootTransform`](gui::GuiRootTransform) based on implementations of
    /// [`GuiRoot::calculate`](gui::GuiRoot::calculate).
    CalculateRoot,
    /// Ensures that components with [`GuiRootTransform`](gui::GuiRootTransform) have a GUI root
    /// component and have no GUI parents.
    ValidateRoot,
    /// Recursively distributes GUI affine transform and size.
    PropagateLayout,
    /// Projects the distributed GUI affine transform and size into 3D world-space points based on
    /// the chosen [`GuiRoot`](gui::GuiRoot).
    CalculateCorners,
}

/// Initializes Hephae GUI common layout systems and the [built-in module](def).
#[derive(Copy, Clone, Default)]
pub struct HephaeGuiPlugin;
impl Plugin for HephaeGuiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GuiLayouts>()
            .init_resource::<GuiRoots>()
            .configure_sets(
                PostUpdate,
                (
                    (HephaeGuiSystems::CalculateRoot, HephaeGuiSystems::ValidateRoot)
                        .before(HephaeGuiSystems::PropagateLayout)
                        .after(CameraUpdateSystem),
                    (HephaeGuiSystems::PropagateLayout, HephaeGuiSystems::CalculateCorners)
                        .chain()
                        .after(CameraUpdateSystem),
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

        #[cfg(feature = "text")]
        app.configure_sets(
            PostUpdate,
            HephaeGuiSystems::PropagateLayout.after(HephaeTextSystems::ComputeStructure),
        );
    }
}
