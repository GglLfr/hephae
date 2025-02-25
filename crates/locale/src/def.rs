//! Defines all the necessary components for localization to work, namely:
//!
//! - [`Locale`], maps locale keys with (potentially formatted) localized strings.
//! - [`LocaleCollection`], maps locale codes (e.g., `en-US`, `id-ID`) with [`Locale`] asset
//!   handles.
//! - [`LocaleKey`], holds a reference to a locale collection and locale key.
//! - [`LocaleResult`], caches the localized result.

use std::{any::type_name, borrow::Cow, ops::Range, slice::Iter};

use bevy_asset::{ReflectAsset, UntypedAssetId, VisitAssetDependencies, prelude::*};
use bevy_derive::{Deref, DerefMut};
use bevy_ecs::{
    component::ComponentId,
    entity::VisitEntitiesMut,
    prelude::*,
    reflect::{ReflectMapEntities, ReflectVisitEntities, ReflectVisitEntitiesMut},
    world::DeferredWorld,
};
use bevy_reflect::prelude::*;
use bevy_utils::{HashMap, warn_once};
use scopeguard::{Always, ScopeGuard};
use smallvec::SmallVec;
use thiserror::Error;

use crate::arg::LocaleArg;

/// Maps locale keys with (potentially formatted) localized strings. See [`LocaleFmt`] for the
/// syntax.
#[derive(Asset, Reflect, Deref, DerefMut, Debug)]
#[reflect(Asset, Debug)]
pub struct Locale(pub HashMap<String, LocaleFmt>);
impl Locale {
    /// Formats a localization string with the provided arguments into an output [`String`].
    pub fn localize_into(&self, key: impl AsRef<str>, args_src: &[&str], out: &mut String) -> Result<(), LocalizeError> {
        match self.get(key.as_ref()).ok_or(LocalizeError::MissingKey)? {
            LocaleFmt::Unformatted(res) => {
                out.clone_from(res);
                Ok(())
            }
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

    /// Convenient shortcut for [`localize_into`](Self::localize_into) that allocates a new
    /// [`String`].
    #[inline]
    pub fn localize(&self, key: impl AsRef<str>, args_src: &[&str]) -> Result<String, LocalizeError> {
        let mut out = String::new();
        self.localize_into(key, args_src, &mut out)?;
        Ok(out)
    }
}

/// Errors that may arise from [`Locale::localize_into`].
#[derive(Error, Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum LocalizeError {
    /// The locale doesn't contain the localization for the supplied key.
    #[error("The locale doesn't contain the localization for the supplied key.")]
    MissingKey,
    /// Missing argument at the given index.
    #[error("Missing argument at index {0}.")]
    MissingArgument(usize),
}

/// A locale string, either unformatted or formatted.
///
/// # Syntax
///
/// The syntax is similar to a subset of [`format!`]; everything is the same, except that in
/// arguments, only explicitly-indexed positional arguments are supported.
///
/// ```
/// use std::str::FromStr;
///
/// use bevy_utils::HashMap;
/// use hephae_locale::{
///     def::{LocaleFmt, LocalizeError},
///     prelude::*,
/// };
///
/// let a = LocaleFmt::from_str("Hi {0}, this is {1}. {5}...").unwrap();
/// let b = LocaleFmt::from_str("It's nice to meet you {{inside these braces for no reason}}.")
///     .unwrap();
/// LocaleFmt::from_str("Can't use {this}, can't use {that:?}, can't use {} either!").unwrap_err();
///
/// let loc = Locale(HashMap::from_iter([
///     (String::from("greet"), a),
///     (String::from("chitchat"), b),
/// ]));
///
/// // Format the arguments with `Locale::localize`. Unnecessary arguments are ignored.
/// assert_eq!(
///     loc.localize("greet", &[
///         "Joe", "Jane", "these", "are", "ignored", "Hehehe"
///     ])
///     .unwrap(),
///     "Hi Joe, this is Jane. Hehehe..."
/// );
///
/// // Double braces are escaped into single braces.
/// assert_eq!(
///     loc.localize("chitchat", &[]).unwrap(),
///     "It's nice to meet you {inside these braces for no reason}."
/// );
///
/// // Missing key will not panic.
/// assert_eq!(
///     loc.localize("missing", &[]).unwrap_err(),
///     LocalizeError::MissingKey
/// );
///
/// // Neither will missing arguments.
/// assert_eq!(
///     loc.localize("greet", &[]).unwrap_err(),
///     LocalizeError::MissingArgument(0),
/// );
/// ```
#[derive(Reflect, Clone, Debug)]
#[reflect(Debug)]
pub enum LocaleFmt {
    /// Locale string with no arguments.
    Unformatted(String),
    /// Locale string with arguments. It is advisable to use
    /// [`LocaleFmt::from_str`](std::str::FromStr::from_str) to create instead of doing so manually.
    Formatted {
        /// The locale string format, without the positional argument markers.
        format: String,
        /// The pairs of format span and index of the argument to be appended.
        args: Vec<(Range<usize>, usize)>,
    },
}

/// Collection of [`Locale`]s, mapped by their locale codes.
#[derive(Reflect, Debug)]
#[reflect(Asset, Debug)]
pub struct LocaleCollection {
    /// The default locale code to use.
    pub default: String,
    /// The [`Locale`] map.
    pub languages: HashMap<String, Handle<Locale>>,
}

impl Asset for LocaleCollection {}
impl VisitAssetDependencies for LocaleCollection {
    #[inline]
    fn visit_dependencies(&self, visit: &mut impl FnMut(UntypedAssetId)) {
        self.languages.values().for_each(|handle| visit(handle.id().untyped()))
    }
}

/// Firing this event will cause all [`LocaleKey`]s to update their results for the new locale code.
#[derive(Event, Reflect, Clone, Debug)]
pub struct LocaleChangeEvent(pub String);

/// Stores the locale key and the handle to a [`LocaleCollection`] as a component to be processed in
/// the pipeline.
///
/// Using [`Commands::spawn_localized`](crate::cmd::LocCommandsExt::spawn_localized) is advisable.
#[derive(Component, Reflect, Clone, Deref, DerefMut, Debug)]
#[component(on_remove = remove_localize)]
#[require(LocaleResult)]
#[reflect(Component, Debug)]
pub struct LocaleKey {
    /// The locale key to fetch and format from. In case of a missing key, [`LocaleResult::result`]
    /// will be empty and a warning will be printed.
    #[deref]
    pub key: Cow<'static, str>,
    /// The handle to the [`LocaleCollection`].
    pub collection: Handle<LocaleCollection>,
}

fn remove_localize(mut world: DeferredWorld, e: Entity, _: ComponentId) {
    let args = std::mem::take(&mut world.get_mut::<LocaleArgs>(e).unwrap().0);
    world.commands().entity(e).queue(move |e: Entity, world: &mut World| {
        world.entity_mut(e).remove::<LocaleArgs>();
        for arg in args {
            world.despawn(arg);
        }
    });
}

/// Formatted localized string, ready to be used.
///
/// Using [`Commands::spawn_localized`](crate::cmd::LocCommandsExt::spawn_localized) is advisable.
#[derive(Component, Reflect, Clone, Default, Deref, DerefMut, Debug)]
#[reflect(Component, Default, Debug)]
pub struct LocaleResult {
    /// The result string fetched from the [collection](LocaleKey::collection) by a
    /// [key](LocaleKey::key).
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
    mut sources: Query<(Entity, &LocaleSrc<T>, &mut LocaleCache)>,
) {
    for (e, src, mut cache) in &mut sources {
        let cache = cache.bypass_change_detection();
        if !cache.changed {
            continue
        }

        cache.changed = false;

        let Some(locale) = locales.get(cache.locale) else {
            cache.result = None;
            continue
        };

        let result = cache.result.get_or_insert_default();
        result.clear();

        if src.localize_into(locale, result).is_err() {
            result.clear();
            warn_once!("An error occurred while trying to format {} in {e}", type_name::<T>());
        }
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
    #[allow(unsafe_op_in_unsafe_fn)]
    unsafe fn guard<'a, 'this: 'a>(
        spans: &'this mut Vec<&'static str>,
    ) -> ScopeGuard<&'this mut Vec<&'a str>, fn(&mut Vec<&'a str>), Always> {
        // Safety: We only change the lifetime, so the value is valid for both types.
        ScopeGuard::with_strategy(
            std::mem::transmute::<&'this mut Vec<&'static str>, &'this mut Vec<&'a str>>(spans),
            Vec::clear,
        )
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
