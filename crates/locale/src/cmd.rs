use std::borrow::Cow;

use bevy_asset::prelude::*;
use bevy_ecs::prelude::*;
use bevy_utils::all_tuples;
use smallvec::{smallvec, SmallVec};

use crate::def::{LocArg, LocCache, LocSrc, Locales, Localize, LocalizeArgs};

pub trait LocBundle {
    fn spawn(this: Self, commands: Commands) -> SmallVec<[Entity; 4]>;
}

impl<T: LocArg> LocBundle for T {
    #[inline]
    fn spawn(this: Self, mut commands: Commands) -> SmallVec<[Entity; 4]> {
        smallvec![commands
            .spawn((LocSrc(this), LocCache {
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
        handle: Handle<Locales>,
        loc: L,
    ) -> EntityCommands;
}

impl LocCommandsExt for Commands<'_, '_> {
    #[inline]
    fn spawn_localized<L: LocBundle>(
        &mut self,
        bundle: impl Bundle,
        key: impl Into<Cow<'static, str>>,
        handle: Handle<Locales>,
        loc: L,
    ) -> EntityCommands {
        let args = L::spawn(loc, self.reborrow());
        self.spawn((
            bundle,
            Localize {
                key: key.into(),
                collection: handle,
            },
            LocalizeArgs(args),
        ))
    }
}

pub trait LocEntityCommandsExt {
    fn localize<L: LocBundle>(&mut self, key: impl Into<Cow<'static, str>>, handle: Handle<Locales>, loc: L) -> &mut Self;
}

impl LocEntityCommandsExt for EntityCommands<'_> {
    #[inline]
    fn localize<L: LocBundle>(&mut self, key: impl Into<Cow<'static, str>>, handle: Handle<Locales>, loc: L) -> &mut Self {
        let args = L::spawn(loc, self.commands());
        self.insert((
            Localize {
                key: key.into(),
                collection: handle,
            },
            LocalizeArgs(args),
        ))
    }
}
