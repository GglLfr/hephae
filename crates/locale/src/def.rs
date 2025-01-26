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
use bevy_reflect::prelude::*;
use bevy_utils::{warn_once, HashMap};
use scopeguard::{Always, ScopeGuard};
use smallvec::SmallVec;
use thiserror::Error;

use crate::arg::LocaleArg;

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

                let mut last = 0;
                for &(ref range, i) in args {
                    // Some sanity checks in case some users for some reason modify the locales manually.
                    let start = range.start.min(format.len());
                    let end = range.end.min(format.len());
                    last = last.max(end);

                    // All these unwraps shouldn't panic.
                    out.push_str(&format[start..end]);
                    out.push_str(args_src[i]);
                }
                out.push_str(&format[last..]);

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
pub struct LocaleCollection {
    pub default: String,
    pub languages: HashMap<String, Handle<Locale>>,
}

impl Asset for LocaleCollection {}
impl VisitAssetDependencies for LocaleCollection {
    #[inline]
    fn visit_dependencies(&self, visit: &mut impl FnMut(UntypedAssetId)) {
        self.languages.values().for_each(|handle| visit(handle.id().untyped()))
    }
}

#[derive(Event, Reflect, Clone)]
pub struct LocaleChangeEvent(pub String);

#[derive(Component, Reflect, Clone, Deref, DerefMut)]
#[component(on_remove = remove_localize)]
#[require(LocaleResult)]
#[reflect(Component)]
pub struct LocaleKey {
    #[deref]
    pub key: Cow<'static, str>,
    pub collection: Handle<LocaleCollection>,
}

fn remove_localize(mut world: DeferredWorld, e: Entity, _: ComponentId) {
    let args = std::mem::take(&mut world.get_mut::<LocaleArgs>(e).unwrap().0);
    world.commands().queue(move |world: &mut World| {
        world.entity_mut(e).remove::<LocaleArgs>();
        for arg in args {
            world.despawn(arg);
        }
    });
}

#[derive(Component, Reflect, Clone, Default, Deref, DerefMut)]
#[reflect(Component, Default)]
pub struct LocaleResult {
    #[deref]
    pub result: String,
    #[reflect(ignore)]
    changed: bool,
    #[reflect(ignore)]
    locale: AssetId<Locale>,
}

#[derive(Component, Reflect, Clone, VisitEntitiesMut)]
#[reflect(Component, MapEntities, VisitEntities, VisitEntitiesMut)]
pub(crate) struct LocaleArgs(pub SmallVec<[Entity; 4]>);
impl<'a> IntoIterator for &'a LocaleArgs {
    type Item = <Self::IntoIter as Iterator>::Item;
    type IntoIter = Iter<'a, Entity>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

#[derive(Component, Reflect, Deref)]
#[require(LocaleCache)]
#[reflect(Component)]
pub(crate) struct LocaleSrc<T: LocaleArg>(pub T);

#[derive(Component, Default)]
pub(crate) struct LocaleCache {
    pub result: Option<String>,
    pub locale: AssetId<Locale>,
    pub changed: bool,
}

pub(crate) fn update_locale_asset(
    mut collection_events: EventReader<AssetEvent<LocaleCollection>>,
    mut locale_events: EventReader<AssetEvent<Locale>>,
    mut change_events: EventReader<LocaleChangeEvent>,
    locales: Res<Assets<LocaleCollection>>,
    mut localize_query: Query<(Ref<LocaleKey>, &mut LocaleResult, &LocaleArgs)>,
    mut cache_query: Query<&mut LocaleCache>,
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
        last.get_or_insert_default().clone_from(new_id);
        true
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
                        .languages
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

pub(crate) fn update_locale_cache<T: LocaleArg>(
    locales: Res<Assets<Locale>>,
    mut sources: Query<(&LocaleSrc<T>, &mut LocaleCache)>,
) {
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
    mut result: Query<(Entity, &LocaleKey, &mut LocaleResult, &LocaleArgs)>,
    cache_query: Query<&LocaleCache>,
    mut arguments: Local<Vec<&'static str>>,
) {
    /// Delegates [`std::mem::transmute`] to shrink the vector element's lifetime, but with
    /// invariant mutable reference lifetime to the vector so it may not be accessed while the
    /// guard is active.
    ///
    /// # Safety:
    /// - The guard must **not** be passed anywhere else. Ideally, you'd want to immediately
    ///   dereference it just to make sure.
    /// - The drop glue of the guard must be called, i.e., [`std::mem::forget`] may not be called.
    ///   This is to ensure the `'a` lifetime objects are cleared out.
    #[inline]
    unsafe fn guard<'a, 'this: 'a>(
        spans: &'this mut Vec<&'static str>,
    ) -> ScopeGuard<&'this mut Vec<&'a str>, fn(&mut Vec<&'a str>), Always> {
        // Safety: We only change the lifetime, so the value is valid for both types.
        ScopeGuard::with_strategy(std::mem::transmute(spans), Vec::clear)
    }

    // Safety: The guard is guaranteed not to be dropped early since it's immediately dereferenced.
    let arguments = &mut **unsafe { guard(&mut arguments) };
    'outer: for (e, loc, result, args) in &mut result {
        if !result.changed {
            continue 'outer
        };

        // Alert `Changed<T>` so systems can listen to it.
        let result = result.into_inner();
        result.changed = false;

        // Don't `warn!`; assume the locale asset hasn't loaded yet.
        let Some(locale) = locales.get(result.locale) else {
            result.clear();
            continue 'outer
        };

        arguments.clear();
        for &arg in args {
            // Don't use `Query::iter_many_mut` here to preserve argument index.
            // The aforementioned method will skip despawned entities which is unfavorable.
            let Ok(cache) = cache_query.get(arg) else {
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
