#![allow(internal_features)]
#![cfg_attr(docsrs, feature(rustdoc_internals))]
#![doc = include_str!("../README.md")]
#![cfg_attr(doc, deny(missing_docs))]

pub mod cmd;
pub mod def;
pub mod loader;

pub mod prelude {}

pub mod plugin {
    use std::{borrow::Cow, marker::PhantomData};

    use bevy_app::{prelude::*, PluginGroupBuilder};
    use bevy_asset::prelude::*;
    use hephae_utils::derive::plugin_conf;

    use crate::{
        def::{LocArg, LocSrc, Locale, LocaleFmt, Locales, Localize, LocalizeArgs},
        loader::{LocaleLoader, LocalesLoader},
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
                                .register_type::<Localize>()
                                .register_type::<LocalizeArgs>()
                                .init_asset::<Locale>()
                                .register_asset_reflect::<Locale>()
                                .register_asset_loader(LocaleLoader)
                                .init_asset::<Locales>()
                                .register_asset_reflect::<Locales>()
                                .register_asset_loader(LocalesLoader);
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
            app.register_type::<LocSrc<T>>();
        }
    }
}
