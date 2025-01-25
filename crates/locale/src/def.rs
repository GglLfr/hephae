use std::{borrow::Cow, ops::Range, slice::Iter};

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
use bevy_utils::{warn_once, HashMap};
use smallvec::SmallVec;
use thiserror::Error;

#[derive(Asset, Reflect, Deref, DerefMut)]
#[reflect(Asset)]
pub struct Locale(pub HashMap<String, LocaleFmt>);
impl Locale {
    pub fn localize_into(&self, key: impl AsRef<str>, args_src: &[&str], out: &mut String) -> Result<(), LocalizeError> {
        match self.get(key.as_ref()).ok_or(LocalizeError::MissingKey)? {
            LocaleFmt::Unformatted(res) => Ok(out.clone_from(res)),
            LocaleFmt::Formatted { format, args } => {
                let len = args.iter().try_fold(0, |mut len, &(ref range, i)| {
                    len += range.end - range.start;
                    len += args_src.get(i).ok_or(LocalizeError::MissingArgument(i))?.len();
                    Ok(len)
                })?;

                out.clear();
                out.reserve_exact(len);

                for &(ref range, i) in args {
                    // Some sanity checks in case some users for some reason modify the locales manually.
                    let start = range.start.min(format.len());
                    let end = range.end.min(format.len());

                    // All these unwraps shouldn't panic.
                    out.push_str(&format[start..end]);
                    out.push_str(args_src[i]);
                }

                Ok(())
            }
        }
    }

    #[inline]
    pub fn localize(&self, key: impl AsRef<str>, args_src: &[&str]) -> Result<String, LocalizeError> {
        let mut out = String::new();
        self.localize_into(key, args_src, &mut out)?;
        Ok(out)
    }
}

#[derive(Error, Debug, Copy, Clone)]
pub enum LocalizeError {
    #[error("The locale doesn't contain the localization for the supplied key.")]
    MissingKey,
    #[error("Missing argument at index {0}.")]
    MissingArgument(usize),
}

#[derive(Clone, Reflect)]
pub enum LocaleFmt {
    Unformatted(String),
    Formatted {
        format: String,
        args: Vec<(Range<usize>, usize)>,
    },
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

#[derive(Event, Reflect, Clone)]
pub struct LocaleChangeEvent(pub String);

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
#[require(LocalizeResult)]
#[reflect(Component)]
pub struct Localize {
    #[deref]
    pub key: Cow<'static, str>,
    pub collection: Handle<Locales>,
}

fn remove_localize(mut world: DeferredWorld, e: Entity, _: ComponentId) {
    let args = std::mem::take(&mut world.get_mut::<LocalizeArgs>(e).unwrap().0);
    world.commands().queue(move |world: &mut World| {
        world.entity_mut(e).remove::<LocalizeArgs>();
        for arg in args {
            world.despawn(arg);
        }
    });
}

#[derive(Component, Reflect, Clone, Default, Deref, DerefMut)]
#[reflect(Component, Default)]
pub struct LocalizeResult {
    #[deref]
    pub result: String,
    #[reflect(ignore)]
    changed: bool,
    #[reflect(ignore)]
    locale: AssetId<Locale>,
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
pub(crate) struct LocCache {
    pub result: Option<String>,
    pub locale: AssetId<Locale>,
    pub changed: bool,
}

pub(crate) fn update_locale_asset(
    mut collection_events: EventReader<AssetEvent<Locales>>,
    mut locale_events: EventReader<AssetEvent<Locale>>,
    mut change_events: EventReader<LocaleChangeEvent>,
    locales: Res<Assets<Locales>>,
    mut localize_query: Query<(Ref<Localize>, &mut LocalizeResult, &LocalizeArgs)>,
    mut cache_query: Query<&mut LocCache>,
    mut last: Local<Option<String>>,
) {
    let new_id = (!change_events.is_empty()).then(|| {
        let mut iter = change_events.read();
        let mut last = iter.next().expect("`events.is_empty()` returned false");

        for next in iter {
            last = next;
        }

        &last.0
    });

    let mut all_change = if let Some(new_id) = new_id {
        if let Some(ref mut last) = *last {
            if &*last == new_id {
                false
            } else {
                last.clone_from(new_id);
                true
            }
        } else {
            *last = Some(new_id.clone());
            true
        }
    } else {
        false
    };

    // Events happening to both of these asset types are very unlikely. Always assume it's a change to
    // save maintainers from severe migraines.
    if !collection_events.is_empty() || !locale_events.is_empty() {
        collection_events.clear();
        locale_events.clear();
        all_change = true;
    }

    for (loc, mut result, args) in &mut localize_query {
        if all_change || loc.is_changed() {
            let locale_id = locales
                .get(&loc.collection)
                .and_then(|collection| {
                    collection
                        .locales
                        .get(new_id.unwrap_or(last.as_ref().unwrap_or(&collection.default)))
                })
                .map(Handle::id)
                .unwrap_or_default();

            for &e in args {
                // Don't use `Query::iter_many_mut` here to preserve argument index.
                // The aforementioned method will skip despawned entities which is unfavorable.
                let Ok(mut cache) = cache_query.get_mut(e) else { continue };

                if all_change {
                    // Signal the cache for refreshing.
                    let cache = cache.bypass_change_detection();
                    cache.changed = true;
                    cache.locale = locale_id;
                }
            }

            // Mark as changed without alerting `Changed<T>` temporarily.
            let result = result.bypass_change_detection();
            result.changed = true;
            result.locale = locale_id;
        }
    }
}

pub(crate) fn update_locale_cache<T: LocArg>(locales: Res<Assets<Locale>>, mut sources: Query<(&LocSrc<T>, &mut LocCache)>) {
    for (src, mut cache) in &mut sources {
        let cache = cache.bypass_change_detection();
        if !cache.changed {
            continue
        }

        cache.changed = false;

        let Some(locale) = locales.get(cache.locale) else {
            cache.result = None;
            continue
        };

        src.localize_into(locale, cache.result.get_or_insert_default());
    }
}

pub(crate) fn update_locale_result(
    locales: Res<Assets<Locale>>,
    mut result: Query<(Entity, &Localize, &mut LocalizeResult, &LocalizeArgs)>,
    cache_query: Query<&LocCache>,
    mut arguments: Local<Vec<&'static str>>,
) {
    // Safety:
    // - We only change the lifetime, so the value is valid for both types.
    // - `scopeguard` guarantees that any element in this vector is dropped when this function
    // finishes, so the local anonymous references aren't leaked out.
    // - The scope guard is guaranteed not to be dropped early since it's immediately dereferenced.
    let arguments = &mut **scopeguard::guard(
        unsafe {
            std::mem::transmute::<
                // Write out the input type here to guard against accidental unsynchronized type change.
                &mut Vec<&'static str>,
                &mut Vec<&str>,
            >(&mut arguments)
        },
        Vec::clear,
    );

    'outer: for (e, loc, result, args) in &mut result {
        if !result.changed {
            continue 'outer
        };

        let result = result.into_inner();
        result.changed = false;

        let Some(locale) = locales.get(result.locale) else {
            warn_once!("Locale {} missing for entity {e}!", result.locale);

            result.clear();
            continue 'outer
        };

        arguments.clear();
        for &arg in args {
            // Don't use `Query::iter_many_mut` here to preserve argument index.
            // The aforementioned method will skip despawned entities which is unfavorable.
            let Ok(cache) = cache_query.get(e) else {
                warn_once!("Locale argument {arg} missing for entity {e}");

                result.clear();
                continue 'outer
            };

            let Some(ref result) = cache.result else {
                warn_once!("Locale argument {arg} failed to localize for entity {e}");

                result.clear();
                continue 'outer
            };

            arguments.push(result);
        }

        if let Err(error) = locale.localize_into(&loc.key, arguments, result) {
            warn_once!("Couldn't localize {e}: {error}");
            result.clear();
        }
    }
}
