//! Defines [`LocaleTarget`] and [`LocaleArg`], both configurable by
//! [`LocaleTargetPlugin`](crate::LocaleTargetPlugin) and
//! [`LocaleArgPlugin`](crate::LocaleArgPlugin), respectively.
//!
//! See each type-level documentations for more information.

use std::{
    borrow::Cow,
    fmt::{Error as FmtError, Write},
};

use bevy::{ecs::component::Mutable, prelude::*, reflect::Reflectable};

use crate::def::{Locale, LocaleFmt, LocaleResult};

/// Components that are localizable. For example, this may be a text widget component. You may
/// configure Hephae to register this type for updating using
/// [`LocaleTargetPlugin`](crate::LocaleTargetPlugin).
pub trait LocaleTarget: Component<Mutability = Mutable> {
    /// Receiver for localized strings result. Calling [`str::clone_into`] is recommended.
    fn update(&mut self, src: &str);
}

pub(crate) fn localize_target<T: LocaleTarget>(mut query: Query<(&mut T, &LocaleResult), Changed<LocaleResult>>) {
    for (mut target, src) in &mut query {
        target.update(src);
    }
}

/// Locale arguments that may be used in positional format locale templates. You may configure
/// Hephae to register this argument using [`LocaleArgPlugin`](crate::LocaleArgPlugin).
pub trait LocaleArg: 'static + FromReflect + Reflectable + Send + Sync {
    /// Extracts this argument into a writable string.
    fn localize_into(&self, locale: &Locale, out: &mut impl Write) -> Result<(), FmtError>;

    /// Convenient shortcut for [`localize_into`](LocaleArg::localize_into) that allocates a new
    /// [`String`].
    #[inline]
    fn localize(&self, locale: &Locale) -> Result<String, FmtError> {
        let mut out = String::new();
        self.localize_into(locale, &mut out)?;

        Ok(out)
    }
}

impl LocaleArg for &'static str {
    #[inline]
    fn localize_into(&self, _: &Locale, out: &mut impl Write) -> Result<(), FmtError> {
        out.write_str(self)
    }
}

impl LocaleArg for String {
    #[inline]
    fn localize_into(&self, _: &Locale, out: &mut impl Write) -> Result<(), FmtError> {
        out.write_str(self)
    }
}

impl LocaleArg for Cow<'static, str> {
    #[inline]
    fn localize_into(&self, _: &Locale, out: &mut impl Write) -> Result<(), FmtError> {
        out.write_str(self)
    }
}

impl LocaleArg for u8 {
    #[inline]
    fn localize_into(&self, _: &Locale, out: &mut impl Write) -> Result<(), FmtError> {
        write!(out, "{self}")
    }
}

impl LocaleArg for u16 {
    #[inline]
    fn localize_into(&self, _: &Locale, out: &mut impl Write) -> Result<(), FmtError> {
        write!(out, "{self}")
    }
}

impl LocaleArg for u32 {
    #[inline]
    fn localize_into(&self, _: &Locale, out: &mut impl Write) -> Result<(), FmtError> {
        write!(out, "{self}")
    }
}

impl LocaleArg for u64 {
    #[inline]
    fn localize_into(&self, _: &Locale, out: &mut impl Write) -> Result<(), FmtError> {
        write!(out, "{self}")
    }
}

impl LocaleArg for u128 {
    #[inline]
    fn localize_into(&self, _: &Locale, out: &mut impl Write) -> Result<(), FmtError> {
        write!(out, "{self}")
    }
}

impl LocaleArg for i8 {
    #[inline]
    fn localize_into(&self, _: &Locale, out: &mut impl Write) -> Result<(), FmtError> {
        write!(out, "{self}")
    }
}

impl LocaleArg for i16 {
    #[inline]
    fn localize_into(&self, _: &Locale, out: &mut impl Write) -> Result<(), FmtError> {
        write!(out, "{self}")
    }
}

impl LocaleArg for i32 {
    #[inline]
    fn localize_into(&self, _: &Locale, out: &mut impl Write) -> Result<(), FmtError> {
        write!(out, "{self}")
    }
}

impl LocaleArg for i64 {
    #[inline]
    fn localize_into(&self, _: &Locale, out: &mut impl Write) -> Result<(), FmtError> {
        write!(out, "{self}")
    }
}

impl LocaleArg for i128 {
    #[inline]
    fn localize_into(&self, _: &Locale, out: &mut impl Write) -> Result<(), FmtError> {
        write!(out, "{self}")
    }
}

impl LocaleArg for f32 {
    #[inline]
    fn localize_into(&self, _: &Locale, out: &mut impl Write) -> Result<(), FmtError> {
        write!(out, "{self:.2}")
    }
}

impl LocaleArg for f64 {
    #[inline]
    fn localize_into(&self, _: &Locale, out: &mut impl Write) -> Result<(), FmtError> {
        write!(out, "{self:.2}")
    }
}

/// A [`LocaleArg`] that fetches a key from a [`Locale`].
///
/// # Caveat
///
/// This only supports [unformatted](LocaleFmt::Unformatted) strings at the moment.
#[derive(Component, Reflect, Clone, Deref, DerefMut, Debug)]
#[reflect(Component, Debug)]
pub struct LocalizeBy(pub Cow<'static, str>);

impl LocaleArg for LocalizeBy {
    #[inline]
    fn localize_into(&self, locale: &Locale, out: &mut impl Write) -> Result<(), FmtError> {
        let Some(LocaleFmt::Unformatted(res)) = locale.get(&***self) else {
            return Err(FmtError);
        };

        write!(out, "{res}")
    }
}
