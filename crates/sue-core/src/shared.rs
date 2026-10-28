//! Shared, copy-on-write buffers. A decoded video frame, a sample library or
//! a texture is often read by several features and threads at once (the
//! timeline, a render job, an exporter). [`Shared`] hands out cheap handles
//! to one buffer, and copies it only when someone writes while others still
//! hold it, so the copy is visible in the code, never hidden.

use std::fmt;
use std::ops::Deref;
use std::sync::Arc;

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

    /// The buffer, if this is the only handle.
    pub fn get_mut(&mut self) -> Option<&mut T> {
        Arc::get_mut(&mut self.0)
    }
}

impl<T: Clone> Shared<T> {
    /// The buffer, mutably. Copies it first if other handles exist.
    pub fn make_mut(&mut self) -> &mut T {
        Arc::make_mut(&mut self.0)
    }

    /// The buffer itself, copied only if other handles exist.
    #[must_use]
    pub fn into_inner(self) -> T {
        Arc::try_unwrap(self.0).unwrap_or_else(|shared| (*shared).clone())
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
}
