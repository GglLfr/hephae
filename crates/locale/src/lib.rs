#![allow(internal_features)]
#![cfg_attr(docsrs, feature(rustdoc_internals))]
#![doc = include_str!("../README.md")]
#![cfg_attr(doc, deny(missing_docs))]

use bevy_ecs::prelude::*;

pub mod arg;
pub mod cmd;
pub mod def;
pub mod loader;

pub mod prelude {
    pub use crate::{
        arg::{LocaleTarget, LocalizeBy},
        cmd::{LocBundle as _, LocCommandsExt as _, LocEntityCommandsExt as _},
        def::{Locale, LocaleCollection, LocaleKey, LocaleResult},
    };
}

pub mod plugin {
    use std::{borrow::Cow, marker::PhantomData};

    use bevy_app::{prelude::*, PluginGroupBuilder};
    use bevy_asset::prelude::*;
    use bevy_ecs::prelude::*;
    use hephae_utils::derive::plugin_conf;

    use crate::{
        arg::{localize_target, LocaleArg, LocaleTarget, LocalizeBy},
        def::{
            update_locale_asset, update_locale_cache, update_locale_result, Locale, LocaleArgs, LocaleChangeEvent,
            LocaleCollection, LocaleFmt, LocaleKey, LocaleResult, LocaleSrc,
        },
        loader::{LocaleCollectionLoader, LocaleLoader},
        HephaeLocaleSystems,
    };

    plugin_conf! {
        /// [`LocaleArg`]s you can pass to [`locales`] to conveniently configure them in one go.
        pub trait ArgConf for LocaleArg, T => locale_arg::<T>()
    }

    plugin_conf! {
        /// [`LocaleTarget`]s you can pass to [`locales`] to conveniently configure them in one go.
        pub trait TargetConf for LocaleTarget, T => locale_target::<T>()
    }

    #[inline]
    pub fn locales<C: ArgConf, L: TargetConf>() -> impl PluginGroup {
        struct LocaleGroup<C: ArgConf, L: TargetConf>(PhantomData<(C, L)>);
        impl<C: ArgConf, L: TargetConf> PluginGroup for LocaleGroup<C, L> {
            #[inline]
            fn build(self) -> PluginGroupBuilder {
                let mut builder = PluginGroupBuilder::start::<Self>()
                    .add(|app: &mut App| {
                        app.register_type::<LocaleFmt>()
                            .register_type::<LocaleKey>()
                            .register_type::<LocaleResult>()
                            .register_type::<LocaleArgs>()
                            .init_asset::<Locale>()
                            .register_asset_reflect::<Locale>()
                            .register_asset_loader(LocaleLoader)
                            .init_asset::<LocaleCollection>()
                            .register_asset_reflect::<LocaleCollection>()
                            .register_asset_loader(LocaleCollectionLoader)
                            .add_event::<LocaleChangeEvent>()
                            .register_type::<LocaleChangeEvent>()
                            .configure_sets(
                                PostUpdate,
                                (
                                    HephaeLocaleSystems::UpdateLocaleAsset,
                                    HephaeLocaleSystems::UpdateLocaleCache,
                                    HephaeLocaleSystems::UpdateLocaleResult,
                                    HephaeLocaleSystems::LocalizeTarget,
                                )
                                    .chain(),
                            )
                            .add_systems(
                                PostUpdate,
                                (
                                    update_locale_asset.in_set(HephaeLocaleSystems::UpdateLocaleAsset),
                                    update_locale_result.in_set(HephaeLocaleSystems::UpdateLocaleResult),
                                ),
                            );
                    })
                    .add(locale_arg::<&'static str>())
                    .add(locale_arg::<String>())
                    .add(locale_arg::<Cow<'static, str>>())
                    .add(locale_arg::<LocalizeBy>());

                builder = C::build(builder);
                L::build(builder)
            }
        }

        LocaleGroup::<C, L>(PhantomData)
    }

    #[inline]
    pub fn locale_arg<T: LocaleArg>() -> impl Plugin {
        |app: &mut App| {
            app.register_type::<LocaleSrc<T>>().add_systems(
                PostUpdate,
                update_locale_cache::<T>.in_set(HephaeLocaleSystems::UpdateLocaleCache),
            );
        }
    }

    #[inline]
    pub fn locale_target<T: LocaleTarget>() -> impl Plugin {
        |app: &mut App| {
            app.add_systems(PostUpdate, localize_target::<T>.in_set(HephaeLocaleSystems::LocalizeTarget));
        }
    }
}

/// Labels assigned to Hephae systems that are added to [`PostUpdate`], responsible over all
/// localizations.
#[derive(SystemSet, Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum HephaeLocaleSystems {
    UpdateLocaleAsset,
    UpdateLocaleCache,
    UpdateLocaleResult,
    LocalizeTarget,
}
