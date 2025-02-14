use std::{
    mem::ManuallyDrop,
    ops::{Deref, DerefMut},
};

use bevy_ecs::prelude::*;

pub trait ComponentOptExt<'a>
where
    <Self::Data as Deref>::Target: 'static + Component + Sized,
{
    type Data: 'a + Deref;

    fn get_or_else(
        self,
        provider: impl FnOnce() -> <Self::Data as Deref>::Target,
        commands: EntityCommands<'a>,
    ) -> RefOrSpawn<'a, Self::Data>;
}

impl<'a, T: 'a + Deref<Target: 'static + Component + Sized>> ComponentOptExt<'a> for Option<T> {
    type Data = T;

    #[inline]
    fn get_or_else(
        self,
        provider: impl FnOnce() -> <Self::Data as Deref>::Target,
        commands: EntityCommands<'a>,
    ) -> RefOrSpawn<'a, Self::Data> {
        RefOrSpawn(match self {
            Some(component) => RefOrSpawnInner::Ref(component),
            None => RefOrSpawnInner::Spawn(commands, ManuallyDrop::new(provider())),
        })
    }
}

pub struct RefOrSpawn<'a, T: 'a + Deref<Target: Component + Sized>>(RefOrSpawnInner<'a, T>);
impl<'a, T: 'a + Deref<Target: Component + Sized>> Drop for RefOrSpawn<'a, T> {
    fn drop(&mut self) {
        if let RefOrSpawnInner::Spawn(ref mut commands, ref component) = self.0 {
            commands.insert(unsafe { (component as *const ManuallyDrop<T::Target> as *const T::Target).read() });
        }
    }
}

impl<'a, T: 'a + Deref<Target: Component + Sized>> Deref for RefOrSpawn<'a, T> {
    type Target = T::Target;

    #[inline]
    fn deref(&self) -> &Self::Target {
        match self.0 {
            RefOrSpawnInner::Ref(ref val) => val,
            RefOrSpawnInner::Spawn(.., ref val) => val,
        }
    }
}

impl<'a, T: 'a + DerefMut<Target: Component + Sized>> DerefMut for RefOrSpawn<'a, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self.0 {
            RefOrSpawnInner::Ref(ref mut val) => val,
            RefOrSpawnInner::Spawn(.., ref mut val) => val,
        }
    }
}

enum RefOrSpawnInner<'a, T: 'a + Deref<Target: Component + Sized>> {
    Ref(T),
    Spawn(EntityCommands<'a>, ManuallyDrop<T::Target>),
}
