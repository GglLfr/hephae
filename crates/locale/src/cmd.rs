//! Defines extensions for [`Commands`] and [`EntityCommands`] for convenient localized entity
//! spawning.

use std::borrow::Cow;

use bevy_asset::prelude::*;
use bevy_ecs::prelude::*;
use bevy_utils::all_tuples;
use smallvec::{smallvec, SmallVec};

use crate::{
    arg::LocaleArg,
    def::{LocaleArgs, LocaleCache, LocaleCollection, LocaleKey, LocaleSrc},
};

/// A set of [`LocaleArg`] objects, implemented for tuples.
pub trait LocBundle {
    #[doc(hidden)]
    fn spawn(this: Self, commands: Commands) -> SmallVec<[Entity; 4]>;
}

macro_rules! impl_loc_bundle {
    ((T0, t0)) => {
        #[cfg_attr(docsrs, doc(fake_variadic))]
        impl<T0: LocBundle> LocBundle for (T0,) {
            #[allow(unused)]
            fn spawn((t0,): Self, mut commands: Commands) -> SmallVec<[Entity; 4]> {
                T0::spawn(t0, commands.reborrow())
            }
        }
    };
    ($(($T:ident, $t:ident)),*) => {
        #[doc(hidden)]
        impl<$($T: LocBundle),*> LocBundle for ($($T,)*) {
            #[allow(unused)]
            fn spawn(($($t,)*): Self, mut commands: Commands) -> SmallVec<[Entity; 4]> {
                let mut out = SmallVec::new();
                $(out.append(&mut $T::spawn($t, commands.reborrow()));)*
                out
            }
        }
    };
}

all_tuples!(impl_loc_bundle, 0, 15, T, t);

impl<T: LocaleArg> LocBundle for T {
    #[inline]
    fn spawn(this: Self, mut commands: Commands) -> SmallVec<[Entity; 4]> {
        smallvec![commands
            .spawn((LocaleSrc(this), LocaleCache {
                result: None,
                locale: AssetId::default(),
                changed: false,
            }))
            .id()]
    }
}

/// Extension for [`Commands`], allowing users to efficiently spawn entities with localization
/// support.
pub trait LocCommandsExt {
    /// [`Commands::spawn`], but also inserts necessary localization components.
    fn spawn_localized<L: LocBundle>(
        &mut self,
        bundle: impl Bundle,
        key: impl Into<Cow<'static, str>>,
        handle: Handle<LocaleCollection>,
        loc: L,
    ) -> EntityCommands;

    /// [`Commands::spawn_empty`], but also inserts necessary localization components.
    fn spawn_localized_empty<L: LocBundle>(
        &mut self,
        key: impl Into<Cow<'static, str>>,
        handle: Handle<LocaleCollection>,
        loc: L,
    ) -> EntityCommands;
}

impl LocCommandsExt for Commands<'_, '_> {
    #[inline]
    fn spawn_localized<L: LocBundle>(
        &mut self,
        bundle: impl Bundle,
        key: impl Into<Cow<'static, str>>,
        handle: Handle<LocaleCollection>,
        loc: L,
    ) -> EntityCommands {
        let args = L::spawn(loc, self.reborrow());
        self.spawn((
            bundle,
            LocaleKey {
                key: key.into(),
                collection: handle,
            },
            LocaleArgs(args),
        ))
    }

    #[inline]
    fn spawn_localized_empty<L: LocBundle>(
        &mut self,
        key: impl Into<Cow<'static, str>>,
        handle: Handle<LocaleCollection>,
        loc: L,
    ) -> EntityCommands {
        let args = L::spawn(loc, self.reborrow());
        self.spawn((
            LocaleKey {
                key: key.into(),
                collection: handle,
            },
            LocaleArgs(args),
        ))
    }
}

/// Extension for [`EntityCommands`] and [`EntityWorldMut`], allowing users to localize existing
/// entities.
pub trait LocEntityExt {
    /// Inserts necessary localization components.
    fn localize<L: LocBundle>(
        &mut self,
        key: impl Into<Cow<'static, str>>,
        handle: Handle<LocaleCollection>,
        loc: L,
    ) -> &mut Self;
}

impl LocEntityExt for EntityCommands<'_> {
    #[inline]
    fn localize<L: LocBundle>(
        &mut self,
        key: impl Into<Cow<'static, str>>,
        handle: Handle<LocaleCollection>,
        loc: L,
    ) -> &mut Self {
        let args = L::spawn(loc, self.commands());
        self.insert((
            LocaleKey {
                key: key.into(),
                collection: handle,
            },
            LocaleArgs(args),
        ))
    }
}

impl LocEntityExt for EntityWorldMut<'_> {
    #[inline]
    fn localize<L: LocBundle>(
        &mut self,
        key: impl Into<Cow<'static, str>>,
        handle: Handle<LocaleCollection>,
        loc: L,
    ) -> &mut Self {
        let bundle = self.world_scope(|world| {
            let cmd = world.commands();
            let args = L::spawn(loc, cmd);

            (
                LocaleKey {
                    key: key.into(),
                    collection: handle,
                },
                LocaleArgs(args),
            )
        });

        self.insert(bundle);
        self.world_scope(World::flush);
        self
    }
}
