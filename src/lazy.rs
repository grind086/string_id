use std::{cell::UnsafeCell, mem::MaybeUninit, ops::Deref};

use parking_lot::Once;

use crate::StringId;

/// Like a [`std::sync::LazyLock`] for a [`StringId`].
///
/// # Example
///
/// ```
/// # use string_id::LazyStringId;
/// static LAZY_STRING_ID: LazyStringId = LazyStringId::new("Some `&'static str`");
/// # assert_eq!(LAZY_STRING_ID.resolve(), "Some `&'static str`");
/// ```
pub struct LazyStringId {
    string: &'static str,
    once: Once,
    data: UnsafeCell<MaybeUninit<StringId>>,
}

unsafe impl Send for LazyStringId {}
unsafe impl Sync for LazyStringId {}

impl LazyStringId {
    /// Creates a new lazily-initialized [`StringId`].
    pub const fn new(s: &'static str) -> Self {
        Self {
            string: s,
            once: Once::new(),
            data: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }

    /// Returns the [`StringId`] only if it has already been accessed.
    pub fn get(&self) -> Option<StringId> {
        if !self.once.is_completed() {
            return None;
        }

        // SAFETY: The value is set before this line is reached
        Some(*unsafe { self.deref_inner() })
    }

    /// Equivalent to dereferencing, but explicit.
    pub fn force(&self) -> StringId {
        *self.force_ref()
    }

    // NB: We need a reference for the [`Deref`] impl
    fn force_ref(&self) -> &StringId {
        self.once.call_once(|| {
            // SAFETY: This function is executed at most once
            let data = unsafe { &mut *self.data.get() };
            data.write(StringId::new_static(self.string));
        });

        // SAFETY: The value is set before this line is reached
        unsafe { self.deref_inner() }
    }

    /// SAFETY: The caller guarantees that the internal value is initialized.
    #[inline]
    const unsafe fn deref_inner(&self) -> &StringId {
        unsafe { (&*self.data.get()).assume_init_ref() }
    }
}

impl AsRef<StringId> for LazyStringId {
    fn as_ref(&self) -> &StringId {
        self.force_ref()
    }
}

impl Deref for LazyStringId {
    type Target = StringId;
    fn deref(&self) -> &Self::Target {
        self.force_ref()
    }
}

impl core::fmt::Debug for LazyStringId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("LazyStringId")
            .field("string", &self.string)
            .field("value", &self.get())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get() {
        static LAZY: LazyStringId = LazyStringId::new("test: get()");
        assert!(LAZY.get().is_none());
        let id = LAZY.force();
        assert_eq!(LAZY.get(), Some(id));
        println!("{LAZY:?}");
    }
}
