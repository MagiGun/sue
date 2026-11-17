//! Shared, copy-on-write buffers. A decoded video frame, a sample library or
//! a texture is often read by several features and threads at once (the
//! timeline, a render job, an exporter). [`Shared`] hands out cheap handles
//! to one buffer, and copies it only when someone writes while others still
//! hold it, so the copy is visible in the code, never hidden.
//!
//! The pixels of a [`crate::RasterTile2D`] live in one: cloning a raster,
//! an entity or a whole arena is a snapshot that costs a reference count,
//! and the writer pays for one copy only while such a snapshot is alive.
//!
//! Every copy is counted ([`copies`]), so a test can prove that a code
//! path made none.

use std::borrow::Cow;
use std::cell::Cell;
use std::fmt;
use std::ops::Deref;
use std::sync::Arc;

thread_local! {
    /// Copies [`Shared`] made on this thread.
    static COPIES: Cell<u64> = const { Cell::new(0) };
}

/// How many times a [`Shared`] buffer was copied on the calling thread
/// since the thread started: by [`Shared::make_mut`] while other handles
/// existed, or by [`Shared::into_inner`] likewise. Handing out a handle
/// ([`Clone`]) never counts, because it copies nothing.
///
/// The count is per thread, so tests that run side by side do not see
/// each other's copies. Take the difference of two readings.
#[must_use]
pub fn copies() -> u64 {
    COPIES.with(Cell::get)
}

fn count_copy() {
    COPIES.with(|copies| copies.set(copies.get().wrapping_add(1)));
}

/// A reference-counted buffer with copy-on-write.
pub struct Shared<T>(Arc<T>);

impl<T> Shared<T> {
    /// Wraps `value`.
    pub fn new(value: T) -> Self {
        Self(Arc::new(value))
    }

    /// `true` if both handles point at the same buffer.
    #[must_use]
    pub fn ptr_eq(a: &Self, b: &Self) -> bool {
        Arc::ptr_eq(&a.0, &b.0)
    }

    /// How many handles share the buffer.
    #[must_use]
    pub fn holders(&self) -> usize {
        Arc::strong_count(&self.0)
    }

    /// `true` while another handle shares the buffer, which is when a
    /// write would copy it.
    #[must_use]
    pub fn is_shared(&self) -> bool {
        self.holders() > 1
    }

    /// The buffer, if this is the only handle.
    pub fn get_mut(&mut self) -> Option<&mut T> {
        Arc::get_mut(&mut self.0)
    }

    /// A handle that does not keep the buffer alive, for a cache that
    /// must never be the reason a write copies (see [`Self::revive`]).
    pub(crate) fn watch(&self) -> std::sync::Weak<T> {
        Arc::downgrade(&self.0)
    }

    /// Another handle to the buffer `watched` saw, if a handle to it is
    /// still alive.
    pub(crate) fn revive(watched: &std::sync::Weak<T>) -> Option<Self> {
        watched.upgrade().map(Self)
    }
}

impl<T: Clone> Shared<T> {
    /// The buffer, mutably. Copies it first if other handles exist (and
    /// counts that in [`copies`]); the other handles keep the buffer as it
    /// was.
    pub fn make_mut(&mut self) -> &mut T {
        // Copied exactly when another handle shares it. (A watcher of a
        // blank buffer, see `watch`, is no handle: the buffer is then
        // only moved to a new count, its samples stay where they are.)
        if Arc::strong_count(&self.0) > 1 {
            count_copy();
        }
        Arc::make_mut(&mut self.0)
    }

    /// The buffer itself, copied only if other handles exist (and counted
    /// in [`copies`] then).
    #[must_use]
    pub fn into_inner(self) -> T {
        Arc::try_unwrap(self.0).unwrap_or_else(|shared| {
            count_copy();
            (*shared).clone()
        })
    }
}

impl<T> Clone for Shared<T> {
    /// Another handle to the same buffer (no copy).
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<T> Deref for Shared<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

impl<'a, T: Clone> From<&'a Shared<Vec<T>>> for Cow<'a, [T]> {
    /// Borrows the buffer (no copy).
    fn from(shared: &'a Shared<Vec<T>>) -> Self {
        Cow::Borrowed(shared.as_slice())
    }
}

impl<T: PartialEq> PartialEq for Shared<T> {
    fn eq(&self, other: &Self) -> bool {
        Self::ptr_eq(self, other) || *self.0 == *other.0
    }
}

impl<T: fmt::Debug> fmt::Debug for Shared<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<T: Default> Default for Shared<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clones_share_until_someone_writes() {
        let a = Shared::new(vec![1.0_f32; 1024]);
        let mut b = a.clone();
        assert!(Shared::ptr_eq(&a, &b));
        assert_eq!(a.holders(), 2);
        assert!(b.get_mut().is_none());
        b.make_mut()[0] = 2.0;
        assert!(!Shared::ptr_eq(&a, &b));
        assert_eq!((a[0], b[0]), (1.0, 2.0));
        let address = b.as_ptr();
        b.make_mut()[1] = 3.0;
        assert_eq!(b.as_ptr(), address, "a sole holder writes in place");
        assert_eq!(b.into_inner()[1], 3.0);
    }

    #[test]
    fn every_copy_is_counted_and_nothing_else_is() {
        let start = copies();
        let mut a = Shared::new(vec![7_u8; 64]);
        assert!(!a.is_shared());
        a.make_mut()[0] = 1;
        let b = a.clone();
        let c = b.clone();
        assert!(a.is_shared());
        assert_eq!(copies(), start, "handles and sole writes copy nothing");
        a.make_mut()[1] = 2;
        assert_eq!(copies(), start + 1, "a write beside other handles");
        a.make_mut()[2] = 3;
        assert_eq!(copies(), start + 1, "then the buffer is the writer's own");
        assert_eq!((b[0], b[1], b[2]), (1, 7, 7), "the others saw no write");
        let owned = b.into_inner();
        assert_eq!(copies(), start + 2, "taking a buffer others still hold");
        assert_eq!(owned, *c);
        assert!(!c.is_shared());
        let _ = c.into_inner();
        assert_eq!(copies(), start + 2, "the last holder takes it as it is");
    }

    #[test]
    fn a_handle_dropped_on_another_thread_ends_the_sharing() {
        let start = copies();
        let mut a = Shared::new(vec![1_u8; 32]);
        let snapshot = a.clone();
        let sum = std::thread::spawn(move || snapshot.iter().map(|&b| u32::from(b)).sum::<u32>())
            .join()
            .unwrap();
        assert_eq!(sum, 32);
        a.make_mut()[0] = 9;
        assert_eq!(copies(), start, "the reader is gone, so no copy");
    }

    #[test]
    fn a_shared_vec_borrows_as_a_slice() {
        let a = Shared::new(vec![1_u8, 2, 3]);
        let cow: Cow<'_, [u8]> = (&a).into();
        assert!(matches!(cow, Cow::Borrowed([1, 2, 3])));
    }
}
