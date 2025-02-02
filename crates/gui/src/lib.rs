#![allow(internal_features)]
#![cfg_attr(docsrs, feature(rustdoc_internals))]
#![doc = include_str!("../README.md")]
#![cfg_attr(doc, deny(missing_docs))]

pub mod def;
pub mod gui;
pub(crate) mod layout;

use bevy_ecs::prelude::*;

/// Common imports for [`hephae_gui`](crate).
pub mod prelude {
    pub use crate::{
        def::*,
        gui::{GuiLayout, GuiRoot},
    };
}

/// App plugins for [`hephae_gui`](crate).
pub mod plugin {
    use std::marker::PhantomData;

    use bevy_app::{prelude::*, PluginGroupBuilder};
    use bevy_ecs::{component::ComponentId, prelude::*, world::DeferredWorld};
    use bevy_render::camera::CameraUpdateSystem;
    #[cfg(feature = "text")]
    use hephae_text::HephaeTextSystems;
    use hephae_utils::prelude::*;

    #[cfg(feature = "text")]
    use crate::def::UiText;
    use crate::{
        def::{update_text_widget, FromCamera2d, UiCont},
        gui::{GuiLayout, GuiLayouts, GuiRoot, GuiRootSpace, GuiRootTransform, GuiRoots, LayoutCache},
        layout::{calculate_corners, calculate_root, propagate_layout, validate_root},
        HephaeGuiSystems,
    };

    plugin_conf! {
        /// [`GuiLayout`]s you can pass to [`gui`] to conveniently configure them in one go.
        pub trait LayoutConf for GuiLayout, T => gui_layout::<T>()
    }

    plugin_conf! {
        /// [`GuiRoot`]s you can pass to [`gui`] to conveniently configure them in one go.
        pub trait RootConf for GuiRoot, T => gui_root::<T>()
    }

    /// Initializes Hephae GUI common layout systems, the [built-in module](crate::def), and the
    /// provided custom layout engines.
    pub fn gui<L: LayoutConf, R: RootConf>() -> impl PluginGroup {
        struct GuiGroup<L: LayoutConf, R: RootConf>(PhantomData<(L, R)>);
        impl<L: LayoutConf, R: RootConf> PluginGroup for GuiGroup<L, R> {
            fn build(self) -> PluginGroupBuilder {
                fn configure_ui(app: &mut App) {
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
                        );
                }

                let mut builder = PluginGroupBuilder::start::<Self>()
                    .add(configure_ui)
                    .add(gui_layout::<UiCont>())
                    .add(gui_root::<FromCamera2d>());

                #[cfg(feature = "text")]
                {
                    fn configure_ui_text(app: &mut App) {
                        app.add_systems(PostUpdate, update_text_widget.after(HephaeGuiSystems::CalculateCorners))
                            .configure_sets(
                                PostUpdate,
                                HephaeGuiSystems::PropagateLayout.after(HephaeTextSystems::ComputeStructure),
                            );
                    }

                    builder = builder.add(gui_layout::<UiText>());
                    builder = builder.add(configure_ui_text)
                }

                builder = L::build(builder);
                R::build(builder)
            }
        }

        GuiGroup::<L, R>(PhantomData)
    }

    /// Registers a [`GuiLayout`] for custom UI layout mechanism.
    pub fn gui_layout<T: GuiLayout>() -> impl Plugin {
        |app: &mut App| {
            fn hook(mut world: DeferredWorld, e: Entity, _: ComponentId) {
                let mut e = world.entity_mut(e);

                // The `unwrap()` never fails here because `T` requires `LayoutCache`.
                let mut cache = e.get_mut::<LayoutCache>().unwrap();
                **cache = None
            }

            app.register_required_components::<T, LayoutCache>();

            let world = app.world_mut();
            world.register_component_hooks::<T>().on_add(hook).on_remove(hook);
            world.resource_scope(|world, mut layouts: Mut<GuiLayouts>| layouts.register::<T>(world))
        }
    }

    /// Registers a [`GuiRoot`] for custom UI available space and projection.
    pub fn gui_root<T: GuiRoot>() -> impl Plugin {
        |app: &mut App| {
            app.register_required_components::<T, GuiRootTransform>()
                .register_required_components::<T, GuiRootSpace>()
                .add_systems(PostUpdate, calculate_root::<T>.in_set(HephaeGuiSystems::CalculateRoot))
                .world_mut()
                .resource_scope(|world, mut roots: Mut<GuiRoots>| roots.register::<T>(world))
        }
    }
}

/// Labels assigned to Hephae systems that are added to [`PostUpdate`](bevy_app::PostUpdate),
/// responsible over all GUI layout calculations.
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
