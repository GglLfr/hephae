#![allow(internal_features)]
#![cfg_attr(any(docsrs, docsrs_dep), feature(rustdoc_internals, doc_cfg))]
#![doc = include_str!("../README.md")]
#![cfg_attr(doc, deny(missing_docs))]

use std::{borrow::Cow, fmt::Debug};

use bevy::{app::PluginGroupBuilder, prelude::*};
use hephae_utils::prelude::*;

use crate::{
    arg::{LocaleArg, LocaleTarget, LocalizeBy, localize_target},
    def::{
        Locale, LocaleArgs, LocaleChangeEvent, LocaleCollection, LocaleFmt, LocaleKey, LocaleResult, LocaleSrc,
        update_locale_asset, update_locale_cache, update_locale_result,
    },
    loader::{LocaleCollectionLoader, LocaleLoader},
};

pub mod arg;
pub mod cmd;
pub mod def;
pub mod loader;

/// Common imports for [`hephae_locale`](crate).
pub mod prelude {
    pub use crate::{
        arg::{LocaleTarget, LocalizeBy},
        cmd::{LocBundle as _, LocCommandsExt as _, LocEntityExt as _},
        def::{Locale, LocaleCollection, LocaleKey, LocaleResult},
    };
}

plugin_conf! {
    /// [`LocaleArg`]s you can pass to [`LocalePlugin`] to conveniently configure them in one go.
    pub trait ArgConf for LocaleArg, T => LocaleArgPlugin::<T>::default()
}

plugin_conf! {
    /// [`LocaleTarget`]s you can pass to [`LocalePlugin`] to conveniently configure them in one go.
    pub trait TargetConf for LocaleTarget, T => LocaleTargetPlugin::<T>::default()
}

plugin_def! {
    /// Configures a custom [`LocaleArg`].
    pub struct LocaleArgPlugin<A: LocaleArg>;
    fn build(&self, app: &mut App) {
        app.register_type::<LocaleSrc<A>>().add_systems(
            PostUpdate,
            update_locale_cache::<A>.in_set(HephaeLocaleSystems::UpdateLocaleCache),
        );
    }
}

plugin_def! {
    /// Configures a custom [`LocaleTarget`].
    pub struct LocaleTargetPlugin<T: LocaleTarget>;
    fn build(&self, app: &mut App) {
        app.add_systems(PostUpdate, localize_target::<T>.in_set(HephaeLocaleSystems::LocalizeTarget));
    }
}

plugin_def! {
    /// Entry point for Hephae's localization plugin, configurable with additional localization argument
    /// types and target localized receivers.
    #[plugin_group]
    pub struct LocalePlugin<A: ArgConf = (), T: TargetConf = ()>;
    fn build(self) -> PluginGroupBuilder {
        let mut builder = PluginGroupBuilder::start::<Self>().add(|app: &mut App| {
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
                        HephaeLocaleSystems::UpdateLocaleCache.ambiguous_with(HephaeLocaleSystems::UpdateLocaleCache),
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
            .add(LocaleArgPlugin::<&'static str>::default())
            .add(LocaleArgPlugin::<String>::default())
            .add(LocaleArgPlugin::<Cow<'static, str>>::default())
            .add(LocaleArgPlugin::<LocalizeBy>::default())
            .add(LocaleArgPlugin::<u8>::default())
            .add(LocaleArgPlugin::<u16>::default())
            .add(LocaleArgPlugin::<u32>::default())
            .add(LocaleArgPlugin::<u64>::default())
            .add(LocaleArgPlugin::<u128>::default())
            .add(LocaleArgPlugin::<i8>::default())
            .add(LocaleArgPlugin::<i16>::default())
            .add(LocaleArgPlugin::<i32>::default())
            .add(LocaleArgPlugin::<i64>::default())
            .add(LocaleArgPlugin::<i128>::default())
            .add(LocaleArgPlugin::<f32>::default())
            .add(LocaleArgPlugin::<f64>::default());

        builder = A::build(builder);
        T::build(builder)
    }
}

/// Labels assigned to Hephae systems that are added to [`PostUpdate`], responsible over all
/// localizations.
#[derive(SystemSet, Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum HephaeLocaleSystems {
    /// Detects locale asset changes (among other things) to notify for cache refreshing.
    UpdateLocaleAsset,
    /// Updates each [`LocaleArg`]s and caches their result.
    UpdateLocaleCache,
    /// Combines each [`LocaleArg`]s into the locale format.
    UpdateLocaleResult,
    /// Brings over the results from [`UpdateLocaleResult`](HephaeLocaleSystems::UpdateLocaleResult)
    /// to the associated [`LocaleTarget`] within the [`Entity`].
    LocalizeTarget,
}
