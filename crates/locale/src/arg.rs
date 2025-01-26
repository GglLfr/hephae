use std::borrow::Cow;

use bevy_derive::{Deref, DerefMut};
use bevy_ecs::prelude::*;
use bevy_reflect::{prelude::*, Reflectable};

use crate::def::{Locale, LocaleFmt, LocaleResult};

pub trait LocaleTarget: Component {
    fn update(&mut self, src: &str);
}

pub(crate) fn localize_target<T: LocaleTarget>(mut query: Query<(&mut T, &LocaleResult), Changed<LocaleResult>>) {
    for (mut target, src) in &mut query {
        target.update(src);
    }
}

pub trait LocaleArg: 'static + FromReflect + Reflectable + Send + Sync {
    fn localize_into(&self, locale: &Locale, out: &mut String) -> Option<()>;

    #[inline]
    fn localize(&self, locale: &Locale) -> Option<String> {
        let mut out = String::new();
        self.localize_into(locale, &mut out)?;

        Some(out)
    }
}

impl LocaleArg for &'static str {
    #[inline]
    fn localize_into(&self, _: &Locale, out: &mut String) -> Option<()> {
        out.push_str(self);
        Some(())
    }
}

impl LocaleArg for String {
    #[inline]
    fn localize_into(&self, _: &Locale, out: &mut String) -> Option<()> {
        out.push_str(self);
        Some(())
    }
}

impl LocaleArg for Cow<'static, str> {
    #[inline]
    fn localize_into(&self, _: &Locale, out: &mut String) -> Option<()> {
        out.push_str(self);
        Some(())
    }
}

#[derive(Component, Reflect, Clone, Deref, DerefMut)]
#[reflect(Component)]
pub struct LocalizeBy(pub Cow<'static, str>);

impl LocaleArg for LocalizeBy {
    #[inline]
    fn localize_into(&self, locale: &Locale, out: &mut String) -> Option<()> {
        let LocaleFmt::Unformatted(res) = locale.get(&***self)? else {
            return None
        };

        out.clone_from(res);
        Some(())
    }
}
