use std::{
    mem::MaybeUninit,
    ops::{Deref, DerefMut},
};

use bevy_ecs::prelude::*;

/// Extension traits for `Option<&T>`, `Option<&mut T>`, and `Option<Ref<T>>`.
pub trait ComponentOption<T: Deref<Target: Component + Sized>>: Sized {
    /// Unwraps the option or inserts a new one provided by a closure to the entity.
    fn get_or_insert_with(
        self,
        commands: EntityCommands,
        insert: impl FnOnce() -> T::Target,
    ) -> RefOrInsert<T>;

    /// Unwraps the option or inserts a new one by-value to the entity.
    #[inline]
    fn get_or_insert(self, commands: EntityCommands, insert: T::Target) -> RefOrInsert<T> {
        self.get_or_insert_with(commands, || insert)
    }

    /// Unwraps the option or inserts a default instance to the entity.
    #[inline]
    fn get_or_default(self, commands: EntityCommands) -> RefOrInsert<T>
    where
        T::Target: Default,
    {
        self.get_or_insert_with(commands, T::Target::default)
    }
}

impl<T: Deref<Target: Component + Sized>> ComponentOption<T> for Option<T> {
    #[inline]
    fn get_or_insert_with(
        self,
        commands: EntityCommands,
        insert: impl FnOnce() -> <T as Deref>::Target,
    ) -> RefOrInsert<T> {
        RefOrInsert(match self {
            Some(val) => RefOrInsertInner::Ref(val),
            None => RefOrInsertInner::Spawn(MaybeUninit::new(insert()), commands),
        })
    }
}

/// Insert-guard returned by [`ComponentOption::get_or_insert_with`].
pub struct RefOrInsert<'a, T: Deref<Target: Component + Sized>>(RefOrInsertInner<'a, T>);
impl<T: Deref<Target: Component + Sized>> Deref for RefOrInsert<'_, T> {
    type Target = T::Target;

    #[inline]
    fn deref(&self) -> &Self::Target {
        match self.0 {
            RefOrInsertInner::Ref(ref val) => val,
            RefOrInsertInner::Spawn(ref val, ..) => unsafe { val.assume_init_ref() },
        }
    }
}

impl<T: DerefMut<Target: Component + Sized>> DerefMut for RefOrInsert<'_, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self.0 {
            RefOrInsertInner::Ref(ref mut val) => val,
            RefOrInsertInner::Spawn(ref mut val, ..) => unsafe { val.assume_init_mut() },
        }
    }
}

impl<T: Deref<Target: Component + Sized>> Drop for RefOrInsert<'_, T> {
    fn drop(&mut self) {
        if let RefOrInsertInner::Spawn(insert, commands) = &mut self.0 {
            commands.insert(unsafe { insert.assume_init_read() });
        }
    }
}

enum RefOrInsertInner<'a, T: Deref<Target: Component + Sized>> {
    Ref(T),
    Spawn(MaybeUninit<T::Target>, EntityCommands<'a>),
}
