#![allow(internal_features)]
#![cfg_attr(docsrs, feature(rustdoc_internals))]
#![doc = include_str!("../README.md")]
#![cfg_attr(doc, deny(missing_docs))]

use bevy_ecs::prelude::*;

pub mod cmd;
pub mod def;
pub mod loader;

pub mod prelude {
    pub use crate::{
        cmd::{LocBundle as _, LocCommandsExt as _, LocEntityCommandsExt as _},
        def::{LocArg, LocKey, LocResult, Locale, Locales},
    };
}

pub mod plugin {
    use std::{borrow::Cow, marker::PhantomData};

    use bevy_app::{prelude::*, PluginGroupBuilder};
    use bevy_asset::prelude::*;
    use bevy_ecs::prelude::*;
    use hephae_utils::derive::plugin_conf;

    use crate::{
        def::{
            update_locale_asset, update_locale_cache, update_locale_result, LocArg, LocArgs, LocKey, LocResult, LocSrc,
            Locale, LocaleChangeEvent, LocaleFmt, Locales,
        },
        loader::{LocaleLoader, LocalesLoader},
        HephaeLocaleSystems,
    };

    plugin_conf! {
        /// [`LocArg`]s you can pass to [`locales`] to conveniently configure them in one go.
        pub trait LocConf for LocArg, T => locale::<T>()
    }

    #[inline]
    pub fn locales<T: LocConf>() -> impl PluginGroup {
        struct LocaleGroup<T: LocConf>(PhantomData<T>);
        impl<T: LocConf> PluginGroup for LocaleGroup<T> {
            #[inline]
            fn build(self) -> PluginGroupBuilder {
                T::build(
                    PluginGroupBuilder::start::<Self>()
                        .add(|app: &mut App| {
                            app.register_type::<LocaleFmt>()
                                .register_type::<LocKey>()
                                .register_type::<LocResult>()
                                .register_type::<LocArgs>()
                                .init_asset::<Locale>()
                                .register_asset_reflect::<Locale>()
                                .register_asset_loader(LocaleLoader)
                                .init_asset::<Locales>()
                                .register_asset_reflect::<Locales>()
                                .register_asset_loader(LocalesLoader)
                                .add_event::<LocaleChangeEvent>()
                                .register_type::<LocaleChangeEvent>()
                                .configure_sets(
                                    PostUpdate,
                                    (
                                        HephaeLocaleSystems::UpdateLocaleAsset,
                                        HephaeLocaleSystems::UpdateLocaleCache,
                                        HephaeLocaleSystems::UpdateLocaleResult,
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
                        .add(locale::<&'static str>())
                        .add(locale::<String>())
                        .add(locale::<Cow<'static, str>>()),
                )
            }
        }

        LocaleGroup::<T>(PhantomData)
    }

    #[inline]
    pub fn locale<T: LocArg>() -> impl Plugin {
        |app: &mut App| {
            app.register_type::<LocSrc<T>>().add_systems(
                PostUpdate,
                update_locale_cache::<T>.in_set(HephaeLocaleSystems::UpdateLocaleCache),
            );
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
}
