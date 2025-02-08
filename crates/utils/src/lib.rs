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

/// Provides `with` and `with_mut`, mainly to cooperate with Loom's mock [`UnsafeCell`].
pub trait UnsafeCellExt {
    /// The `T` in [`UnsafeCell<T>`].
    type T: ?Sized;

    /// Gets an immutable pointer to the wrapped value, applying a closure to it.
    fn with<R>(&self, accept: impl FnOnce(*const Self::T) -> R) -> R;

    /// Gets a mutable pointer to the wrapped value, applying a closure to it.
    fn with_mut<R>(&self, accept: impl FnOnce(*mut Self::T) -> R) -> R;
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
