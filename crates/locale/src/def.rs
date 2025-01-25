use std::{borrow::Cow, ops::Range, slice::Iter, str::FromStr};

use bevy_asset::{prelude::*, ReflectAsset, UntypedAssetId, VisitAssetDependencies};
use bevy_derive::{Deref, DerefMut};
use bevy_ecs::{
    component::ComponentId,
    entity::VisitEntitiesMut,
    prelude::*,
    reflect::{ReflectMapEntities, ReflectVisitEntities, ReflectVisitEntitiesMut},
    world::DeferredWorld,
};
use bevy_reflect::{prelude::*, Reflectable};
use bevy_utils::HashMap;
use smallvec::SmallVec;

#[derive(Asset, Reflect)]
#[reflect(Asset)]
pub struct Locale(pub HashMap<String, LocaleFmt>);

#[derive(Clone, Reflect)]
pub enum LocaleFmt {
    Unformatted(String),
    Formatted {
        format: String,
        args: Vec<(Range<usize>, usize)>,
    },
}

impl FromStr for LocaleFmt {
    type Err = usize;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut chars = s.chars().enumerate();

        #[derive(Copy, Clone)]
        enum State {
            Unescaped,
            PreEscaped(bool),
            Index,
        }

        use State::*;

        let mut format = String::new();
        let mut args = Vec::new();
        let mut range = 0..0;
        let mut state = Unescaped;

        let mut index = 0usize;
        while let Some((i, char)) = chars.next() {
            match char {
                '{' => {
                    state = match state {
                        Unescaped => PreEscaped(false),
                        PreEscaped(false) => {
                            format.push('{');
                            range.end = format.len();

                            Unescaped
                        }
                        PreEscaped(true) | Index => return Err(i),
                    }
                }
                '}' => {
                    state = match state {
                        Unescaped => PreEscaped(true),
                        PreEscaped(false) => return Err(i),
                        PreEscaped(true) => {
                            format.push('}');
                            range.end = format.len();

                            Unescaped
                        }
                        Index => {
                            args.push((range.clone(), index));
                            range.start = range.end;

                            Unescaped
                        }
                    }
                }
                '0'..='9' => match state {
                    Unescaped => {
                        format.push(char);
                        range.end = format.len();
                    }
                    PreEscaped(false) => state = Index,
                    PreEscaped(true) => return Err(i),
                    Index => {
                        index = index
                            .checked_mul(10)
                            .and_then(|index| index.checked_add(char.to_digit(10)? as usize))
                            .ok_or(i)?;
                    }
                },
                _ => match state {
                    Unescaped => {
                        format.push(char);
                        range.end = format.len();
                    }
                    _ => return Err(i),
                },
            }
        }

        Ok(if args.is_empty() {
            Self::Unformatted(format)
        } else {
            Self::Formatted { format, args }
        })
    }
}

#[derive(Reflect)]
#[reflect(Asset)]
pub struct Locales {
    pub default: String,
    pub locales: HashMap<String, Handle<Locale>>,
}

impl Asset for Locales {}
impl VisitAssetDependencies for Locales {
    #[inline]
    fn visit_dependencies(&self, visit: &mut impl FnMut(UntypedAssetId)) {
        self.locales.values().for_each(|handle| visit(handle.id().untyped()))
    }
}

pub trait LocArg: 'static + FromReflect + Reflectable + Send + Sync {
    fn localize_into(&self, locale: &Locale, out: &mut String) -> Option<()>;

    #[inline]
    fn localize(&self, locale: &Locale) -> Option<String> {
        let mut out = String::new();
        self.localize_into(locale, &mut out)?;

        Some(out)
    }
}

impl LocArg for &'static str {
    #[inline]
    fn localize_into(&self, _: &Locale, out: &mut String) -> Option<()> {
        out.push_str(self);
        Some(())
    }
}

impl LocArg for String {
    #[inline]
    fn localize_into(&self, _: &Locale, out: &mut String) -> Option<()> {
        out.push_str(self);
        Some(())
    }
}

impl LocArg for Cow<'static, str> {
    #[inline]
    fn localize_into(&self, _: &Locale, out: &mut String) -> Option<()> {
        out.push_str(self);
        Some(())
    }
}

#[derive(Component, Reflect, Clone, Deref, DerefMut)]
#[component(on_remove = remove_localize)]
#[reflect(Component)]
pub struct Localize(pub Cow<'static, str>);

fn remove_localize(mut world: DeferredWorld, e: Entity, _: ComponentId) {
    let args = std::mem::take(&mut world.get_mut::<LocalizeArgs>(e).unwrap().0);
    world.commands().queue(move |world: &mut World| {
        world.entity_mut(e).remove::<LocalizeArgs>();
        for arg in args {
            world.despawn(arg);
        }
    });
}

#[derive(Component, Reflect, Clone, VisitEntitiesMut)]
#[reflect(Component, MapEntities, VisitEntities, VisitEntitiesMut)]
pub(crate) struct LocalizeArgs(pub SmallVec<[Entity; 4]>);
impl<'a> IntoIterator for &'a LocalizeArgs {
    type Item = <Self::IntoIter as Iterator>::Item;
    type IntoIter = Iter<'a, Entity>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

#[derive(Component, Reflect, Deref)]
#[require(LocCache)]
#[reflect(Component)]
pub(crate) struct LocSrc<T: LocArg>(pub T);

#[derive(Component, Default)]
pub(crate) struct LocCache(pub String);
