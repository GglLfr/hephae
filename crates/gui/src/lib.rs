#![doc = include_str!("../README.md")]
#![cfg_attr(doc, deny(missing_docs))]

pub mod gui;
pub mod hui;
pub mod layout;
pub mod root;

use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use bevy_render::camera::CameraUpdateSystem;

use crate::{gui::GuiLayouts, hui::HuiPlugin, layout::propagate_layout};

pub mod prelude {
    pub use crate::{hui, HephaeGuiPlugin};
}

#[derive(SystemSet, Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum HephaeGuiSystems {
    CalculatePreferredSize,
    PropagateLayout,
    CalculateRoot,
}

#[derive(Copy, Clone, Default)]
pub struct HephaeGuiPlugin;
impl Plugin for HephaeGuiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GuiLayouts>()
            .configure_sets(
                PostUpdate,
                (
                    (HephaeGuiSystems::CalculatePreferredSize, HephaeGuiSystems::PropagateLayout).chain(),
                    HephaeGuiSystems::CalculateRoot.after(CameraUpdateSystem),
                ),
            )
            .add_systems(PostUpdate, propagate_layout.in_set(HephaeGuiSystems::PropagateLayout))
            .add_plugins(HuiPlugin);
    }
}
