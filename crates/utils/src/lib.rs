#![allow(internal_features)]
#![cfg_attr(docsrs, feature(rustdoc_internals))]
#![doc = include_str!("../README.md")]
#![cfg_attr(doc, deny(missing_docs))]

pub use hephae_utils_derive as derive;

pub mod vec_belt;

/// Common imports for [`hephae_utils`](crate).
pub mod prelude {
    pub use crate::{derive::plugin_conf, sync};
}

pub mod sync {
    pub use std::{
        hint::spin_loop as busy_wait,
        sync::{
            atomic::{
                fence as atomic_fence, AtomicBool, AtomicI16, AtomicI32, AtomicI64, AtomicI8, AtomicIsize, AtomicPtr,
                AtomicU16, AtomicU32, AtomicU64, AtomicU8, AtomicUsize, Ordering::*,
            },
            Arc, Barrier, Condvar, Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard, WaitTimeoutResult,
        },
        thread,
    };
}

/// Stable version of [`std::hint::likely`].
#[inline(always)]
pub const fn likely(cond: bool) -> bool {
    1u8.checked_div(if cond { 1 } else { 0 }).is_some()
}

/// Stable version of [`std::hint::unlikely`].
#[inline(always)]
pub const fn unlikely(cond: bool) -> bool {
    1u8.checked_div(if cond { 0 } else { 1 }).is_none()
}

pub trait LikelyResult {
    type Ok;
    type Err;

    fn likely_ok<R>(self, ok: impl FnOnce(Self::Ok) -> R, err: impl FnOnce(Self::Err) -> R) -> R;

    fn likely_err<R>(self, ok: impl FnOnce(Self::Ok) -> R, err: impl FnOnce(Self::Err) -> R) -> R;
}

impl<T, E> LikelyResult for Result<T, E> {
    type Ok = T;
    type Err = E;

    #[inline(always)]
    fn likely_ok<R>(self, ok: impl FnOnce(T) -> R, err: impl FnOnce(E) -> R) -> R {
        if likely(matches!(self, Ok(..))) {
            ok(unsafe { self.unwrap_unchecked() })
        } else {
            err(unsafe { self.unwrap_err_unchecked() })
        }
    }

    #[inline(always)]
    fn likely_err<R>(self, ok: impl FnOnce(T) -> R, err: impl FnOnce(E) -> R) -> R {
        if likely(matches!(self, Err(..))) {
            err(unsafe { self.unwrap_err_unchecked() })
        } else {
            ok(unsafe { self.unwrap_unchecked() })
        }
    }
}
