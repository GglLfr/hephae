use std::{
    mem::MaybeUninit,
    ops::{Deref, DerefMut},
};

use bevy_ecs::prelude::*;

pub trait ComponentOption<T: Deref<Target: Component + Sized>>: Sized {
    fn get_or_insert_with(self, commands: EntityCommands, insert: impl FnOnce() -> T::Target) -> RefOrSpawn<T>;

    #[inline]
    fn get_or_insert(self, commands: EntityCommands, insert: T::Target) -> RefOrSpawn<T> {
        self.get_or_insert_with(commands, || insert)
    }

    #[inline]
    fn get_or_default(self, commands: EntityCommands) -> RefOrSpawn<T>
    where
        T::Target: Default,
    {
        self.get_or_insert_with(commands, T::Target::default)
    }
}

impl<T: Deref<Target: Component + Sized>> ComponentOption<T> for Option<T> {
    #[inline]
    fn get_or_insert_with(self, commands: EntityCommands, insert: impl FnOnce() -> <T as Deref>::Target) -> RefOrSpawn<T> {
        RefOrSpawn(match self {
            Some(val) => RefOrSpawnInner::Ref(val),
            None => RefOrSpawnInner::Spawn(MaybeUninit::new(insert()), commands),
        })
    }
}

pub struct RefOrSpawn<'a, T: Deref<Target: Component + Sized>>(RefOrSpawnInner<'a, T>);
impl<T: Deref<Target: Component + Sized>> Deref for RefOrSpawn<'_, T> {
    type Target = T::Target;

    #[inline]
    fn deref(&self) -> &Self::Target {
        match self.0 {
            RefOrSpawnInner::Ref(ref val) => val,
            RefOrSpawnInner::Spawn(ref val, ..) => unsafe { val.assume_init_ref() },
        }
    }
}

impl<T: DerefMut<Target: Component + Sized>> DerefMut for RefOrSpawn<'_, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self.0 {
            RefOrSpawnInner::Ref(ref mut val) => val,
            RefOrSpawnInner::Spawn(ref mut val, ..) => unsafe { val.assume_init_mut() },
        }
    }
}

impl<T: Deref<Target: Component + Sized>> Drop for RefOrSpawn<'_, T> {
    fn drop(&mut self) {
        if let RefOrSpawnInner::Spawn(insert, commands) = &mut self.0 {
            commands.insert(unsafe { insert.assume_init_read() });
        }
    }
}

enum RefOrSpawnInner<'a, T: Deref<Target: Component + Sized>> {
    Ref(T),
    Spawn(MaybeUninit<T::Target>, EntityCommands<'a>),
}
