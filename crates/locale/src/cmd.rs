use std::borrow::Cow;

use bevy_asset::prelude::*;
use bevy_ecs::prelude::*;
use bevy_utils::all_tuples;
use smallvec::{smallvec, SmallVec};

use crate::{
    arg::LocaleArg,
    def::{LocaleArgs, LocaleCache, LocaleCollection, LocaleKey, LocaleSrc},
};

pub trait LocBundle {
    fn spawn(this: Self, commands: Commands) -> SmallVec<[Entity; 4]>;
}

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

macro_rules! impl_loc_bundle {
    ($(($T:ident, $t:ident)),*) => {
        #[cfg_attr(docsrs, doc(fake_variadic))]
        impl<$($T: LocBundle),*> LocBundle for ($($T,)*) {
            #[allow(unused)]
            fn spawn(($($t,)*): Self, mut commands: Commands) -> SmallVec<[Entity; 4]> {
                let mut out = SmallVec::new();
                $(out.append(&mut $T::spawn($t, commands.reborrow()));)*
                out
            }
        }
    }
}

all_tuples!(impl_loc_bundle, 0, 15, L, l);

pub trait LocCommandsExt {
    fn spawn_localized<L: LocBundle>(
        &mut self,
        bundle: impl Bundle,
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
}

pub trait LocEntityCommandsExt {
    fn localize<L: LocBundle>(
        &mut self,
        key: impl Into<Cow<'static, str>>,
        handle: Handle<LocaleCollection>,
        loc: L,
    ) -> &mut Self;
}

impl LocEntityCommandsExt for EntityCommands<'_> {
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
