#![allow(internal_features)]
#![cfg_attr(any(docsrs, docsrs_dep), feature(rustdoc_internals))]
#![doc = include_str!("../README.md")]
#![cfg_attr(doc, deny(missing_docs))]

pub mod measure;
pub mod node;
pub mod root;
pub mod style;

use bevy_app::{App, PluginGroupBuilder, PostUpdate};
use bevy_ecs::prelude::*;
use bevy_render::camera::CameraUpdateSystem;
use bevy_transform::{TransformSystem, prelude::Transform};
use hephae_utils::prelude::*;

use crate::{
    measure::{ContentSize, Measure, Measurements, on_measure_inserted},
    node::compute_ui_tree,
    prelude::Camera2dRoot,
    root::{UiRoot, UiRootTrns, compute_root_transform},
    style::ui_changed,
};

/// Common imports for [`hephae-ui`](crate).
pub mod prelude {
    pub use crate::{
        node::{Border, ComputedUi, UiCaches},
        root::Camera2dRoot,
        style::{
            AlignContent, AlignItems, AlignSelf, BoxSizing, Display, FlexDirection, FlexWrap, JustifyContent, Overflow,
            Position, Ui, UiBorder, UiSize,
            Val::{self, *},
        },
    };
}

plugin_conf! {
    /// [`Measure`]s you can pass to [`UiPlugin`] to conveniently configure them in one go.
    pub trait MeasureConf for Measure, T => UiMeasurePlugin::<T>::default()
}

plugin_conf! {
    /// [`UiRoot`]s you can pass to [`UiPlugin`] to conveniently configure them in one go.
    pub trait RootConf for UiRoot, T => UiRootPlugin::<T>::default()
}

plugin_def! {
    /// Configures a custom UI leaf node measurer.
    pub struct UiMeasurePlugin<T: Measure>;
    fn build(&self, app: &mut App) {
        app.register_required_components::<T, ContentSize>()
            .add_observer(on_measure_inserted::<T>)
            .world_mut()
            .resource_scope(|world, mut measurements: Mut<Measurements>| {
                measurements.register::<T>(world);
            });
    }
}

plugin_def! {
    /// Configures a custom UI root component.
    pub struct UiRootPlugin<T: UiRoot>;
    fn build(&self, app: &mut App) {
        app.register_required_components::<T, UiRootTrns>()
            .register_required_components::<T, Transform>()
            .add_systems(
                PostUpdate,
                compute_root_transform::<T>.in_set(HephaeUiSystems::ComputeRootTransform),
            );
    }
}

plugin_def! {
    /// Configures Hephae UI in your application. Pass additional user-defined leaf node measurers
    /// and UI roots as pleased.
    #[plugin_group]
    pub struct UiPlugin<M: MeasureConf = (), R: RootConf = ()>;
    fn build(self) -> PluginGroupBuilder {
        let mut builder = PluginGroupBuilder::start::<Self>()
            .add(|app: &mut App| {
                app.init_resource::<Measurements>()
                    .configure_sets(
                        PostUpdate,
                        (
                            (
                                HephaeUiSystems::ComputeRootTransform.after(CameraUpdateSystem),
                                HephaeUiSystems::InvalidateCaches,
                            ),
                            HephaeUiSystems::ComputeUiLayout,
                        )
                            .chain()
                            .before(TransformSystem::TransformPropagate),
                    )
                    .add_systems(
                        PostUpdate,
                        (
                            ui_changed.in_set(HephaeUiSystems::InvalidateCaches),
                            compute_ui_tree.in_set(HephaeUiSystems::ComputeUiLayout),
                        ),
                    );
            })
            .add(UiRootPlugin::<Camera2dRoot>::default());

        builder = M::build(builder);
        R::build(builder)
    }
}

/// Labels for systems added by Hephae UI.
#[derive(SystemSet, Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum HephaeUiSystems {
    /// System in [`PostUpdate`] that calculates transform and available size for each UI root.
    ComputeRootTransform,
    /// System in [`PostUpdate`] that is responsible over invalidating UI layout caches so the
    /// pipeline will recompute them.
    InvalidateCaches,
    /// System in [`PostUpdate`] that calculates every UI node layouts recursively starting from the
    /// root.
    ComputeUiLayout,
}
