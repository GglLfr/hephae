#![allow(internal_features)]
#![cfg_attr(any(docsrs, docsrs_dep), feature(rustdoc_internals))]
#![doc = include_str!("../README.md")]
#![cfg_attr(doc, deny(missing_docs))]

pub mod measure;
pub mod node;
pub mod root;
pub mod style;

use bevy_ecs::prelude::*;

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

/// App plugins for [`hephae_ui`](crate).
pub mod plugin {
    use std::marker::PhantomData;

    use bevy_app::{PluginGroupBuilder, prelude::*};
    use bevy_ecs::prelude::*;
    use bevy_render::camera::CameraUpdateSystem;
    use bevy_transform::prelude::*;
    use hephae_utils::derive::plugin_conf;

    use crate::{
        HephaeUiSystems,
        measure::{ContentSize, Measure, Measurements, on_measure_inserted},
        node::compute_ui_tree,
        root::{Camera2dRoot, UiRoot, UiRootTrns, compute_root_transform},
        style::ui_changed,
    };

    plugin_conf! {
        /// [`Measure`]s you can pass to [`ui_measure`] to conveniently configure them in one go.
        pub trait MeasureConf for Measure, T => ui_measure::<T>()
    }

    plugin_conf! {
        /// [`UiRoot`]s you can pass to [`ui_root`] to conveniently configure them in one go.
        pub trait RootConf for UiRoot, T => ui_root::<T>()
    }

    /// Configures Hephae UI in your application. Pass additional user-defined leaf node measurers
    /// and UI roots as pleased.
    pub fn ui<M: MeasureConf, R: RootConf>() -> impl PluginGroup {
        struct UiGroup<M: MeasureConf, R: RootConf>(PhantomData<(M, R)>);
        impl<M: MeasureConf, R: RootConf> PluginGroup for UiGroup<M, R> {
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
                    .add(ui_root::<Camera2dRoot>());

                builder = M::build(builder);
                R::build(builder)
            }
        }

        UiGroup::<M, R>(PhantomData)
    }

    /// Configures a custom UI leaf node measurer.
    pub fn ui_measure<T: Measure>() -> impl Plugin {
        |app: &mut App| {
            app.register_required_components::<T, ContentSize>()
                .add_observer(on_measure_inserted::<T>)
                .world_mut()
                .resource_scope(|world, mut measurements: Mut<Measurements>| {
                    measurements.register::<T>(world);
                });
        }
    }

    /// Configures a custom UI root component.
    pub fn ui_root<T: UiRoot>() -> impl Plugin {
        |app: &mut App| {
            app.register_required_components::<T, UiRootTrns>()
                .register_required_components::<T, Transform>()
                .add_systems(
                    PostUpdate,
                    compute_root_transform::<T>.in_set(HephaeUiSystems::ComputeRootTransform),
                );
        }
    }
}

/// Labels for systems added by Hephae UI.
#[derive(SystemSet, Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum HephaeUiSystems {
    /// System in [`PostUpdate`](bevy_app::PostUpdate) that calculates transform and available size
    /// for each UI root.
    ComputeRootTransform,
    /// System in [`PostUpdate`](bevy_app::PostUpdate) that is responsible over invalidating UI
    /// layout caches so the pipeline will recompute them.
    InvalidateCaches,
    /// System in [`PostUpdate`](bevy_app::PostUpdate) that calculates every UI node layouts
    /// recursively starting from the root.
    ComputeUiLayout,
}
