use foldhash::fast::FixedState;
use parking_lot::{Mutex, RwLock};
use std::{collections::HashMap, num::NonZeroU32};

use crate::buffer::StringBuffer;

/// A seed to use for the interner's [`FixedState`].
// NB: We need to define this in a constant in order to provide a const constructor for `Interner`.
const STATE_SEED: u64 = 0; // 0 results in `FixedState::default`

/// A multithreaded string interner.
/// 
/// `&'static str` values are not copied into the backing buffer, and the backing buffer does not
/// allocate until non-`'static` values are interned. 
///
/// [`StringId`][crate::StringId] uses its own static interner, but `Interner` may also be used
/// manually. Interned strings are tied to the lifetime of the interner, and their buffers will be
/// dropped with the interner.
pub struct Interner<'a> {
    buffer: Mutex<StringBuffer<'a>>,
    lookup: RwLock<HashMap<&'a str, RawStringId, FixedState>>,
    resolve: RwLock<Vec<&'a str>>,
}

impl<'a> Interner<'a> {
    /// Creates a new, empty `Interner`.
    ///
    /// The interner's string buffer will not allocate until non-`'static` strings are interned.
    pub const fn new() -> Self {
        Interner {
            buffer: Mutex::new(StringBuffer::new()),
            lookup: RwLock::new(HashMap::with_hasher(FixedState::with_seed(STATE_SEED))),
            resolve: RwLock::new(Vec::new()),
        }
    }

    /// The total number of bytes that have been allocated by this interner's string buffer.
    pub fn allocated_bytes(&self) -> usize {
        self.buffer.lock().allocated_bytes()
    }

    /// The number of bytes that can be interned before allocating.
    pub fn reserved_bytes(&self) -> usize {
        self.buffer.lock().reserved_bytes()
    }

    /// The total number of unique strings that are interned in this `Interner`.
    pub fn interned_strings(&self) -> usize {
        self.resolve.read().len()
    }

    /// Looks up the [`RawStringId`] of `s` *without* interning it.
    pub fn get(&self, s: &str) -> Option<RawStringId> {
        self.lookup.read().get(s).copied()
    }

    /// Deduplicates and interns a string, returning a lightweight identifier.
    ///
    /// The returned identifier can be used with [`Self::resolve`] to retrieve the original string.
    pub fn intern(&self, s: &str) -> RawStringId {
        self.intern_with(s, |this| this.buffer.lock().write(s))
    }

    /// Deduplicates and interns a `'static` string, returning a lightweight identifier.
    ///
    /// This is similar to [`Self::intern`], but does not copy the string into the interner's
    /// buffer.
    ///
    /// # Example
    ///
    /// ```
    /// # use string_id::interner::Interner;
    /// let interner = Interner::new();
    ///
    /// # let id =
    /// interner.intern_static("Hello, world!");
    ///
    /// assert_eq!(interner.interned_strings(), 1);
    /// assert_eq!(interner.allocated_bytes(), 0);
    /// # assert_eq!(interner.resolve(id), "Hello, world!");
    /// ```
    pub fn intern_static(&self, s: &'static str) -> RawStringId {
        self.intern_with(s, |_| s)
    }

    /// Resolves the string with the given `id`.
    ///
    /// If the provided `RawStringId` was produced by a different interner, and this function does
    /// not panic, then the returned string is an arbitrary but consistent member of this interner.
    ///
    /// # Panics
    ///
    /// This function may panic if given a [`RawStringId`] produced by a different interner.
    pub fn resolve(&self, id: RawStringId) -> &'a str {
        self.resolve.read()[id.into_index()]
    }

    #[inline(always)]
    fn intern_with(&self, s: &str, save: impl Fn(&Interner<'a>) -> &'a str) -> RawStringId {
        // Happy path - the string is already interned, so we only need a read lock and a hash lookup.
        if let Some(id) = self.lookup.read().get(s) {
            return *id;
        }

        // Otherwise we need to get write locks for everything, and intern the string.
        let mut lookup_lock = self.lookup.write();

        // Check if the string was interned while we were waiting for the lock
        if let Some(id) = lookup_lock.get(s) {
            core::hint::cold_path();
            return *id;
        }

        let mut resolve_lock = self.resolve.write();

        let id = RawStringId::from_index(resolve_lock.len()).expect("interner full");
        let buf_str = save(self);
        lookup_lock.insert(buf_str, id);
        resolve_lock.push(buf_str);

        drop(resolve_lock);
        drop(lookup_lock);

        id
    }
}

impl Default for Interner<'_> {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct RawStringId(NonZeroU32);

impl RawStringId {
    /// Returns the inner `u32` value of the `RawStringId`.
    pub const fn into_inner(self) -> u32 {
        self.0.get() ^ u32::MAX
    }

    fn from_index(index: usize) -> Option<Self> {
        NonZeroU32::new(index as u32 ^ u32::MAX).map(Self)
    }

    fn into_index(self) -> usize {
        self.into_inner() as _
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;

    #[test]
    fn raw_id_niche() {
        assert_eq!(size_of::<RawStringId>(), size_of::<Option<RawStringId>>());
    }

    #[test]
    fn intern_static_does_not_allocate() {
        let interner = Interner::new();

        assert_eq!(interner.interned_strings(), 0);
        assert_eq!(interner.allocated_bytes(), 0);

        interner.intern_static("a");
        assert_eq!(interner.interned_strings(), 1);
        assert_eq!(interner.allocated_bytes(), 0);

        let b0 = interner.intern_static("b");
        assert_eq!(interner.interned_strings(), 2);
        assert_eq!(interner.allocated_bytes(), 0);

        let b1 = interner.intern("b");
        assert_eq!(interner.interned_strings(), 2);
        assert_eq!(interner.allocated_bytes(), 0);
        assert_eq!(b0, b1);
    }

    #[test]
    fn multithreaded() {
        static STRS: &[&str] = &["a", "b", "c", "d"];
        static INTERNER: Interner = Interner::new();

        let handles = (0..16)
            .map(|_| {
                std::thread::spawn(|| STRS.iter().map(|s| INTERNER.intern(s)).collect::<Vec<_>>())
            })
            .collect::<Vec<_>>();

        for handle in handles {
            handle
                .join()
                .unwrap()
                .into_iter()
                .enumerate()
                .for_each(|(idx, sid)| assert_eq!(INTERNER.resolve(sid), STRS[idx]));
        }

        assert_eq!(INTERNER.interned_strings(), STRS.len());
    }

    #[test]
    fn multithreaded_high_contest() {
        static STRS: &[&str] = &["a", "b", "c", "d", "e", "f"];
        static INTERNER: Interner = Interner::new();

        let start = AtomicBool::new(false);
        let handles = std::thread::scope(|s| {
            let handles = (0..20)
                .map(|_| {
                    s.spawn(|| {
                        while !start.load(Ordering::Relaxed) {
                            std::hint::spin_loop()
                        }
                        core::iter::repeat_with(|| STRS.iter())
                            .flatten()
                            .map(|s| INTERNER.intern(s))
                            .take(10)
                            .collect::<Vec<_>>()
                    })
                })
                .collect::<Vec<_>>();
            start.store(true, Ordering::Relaxed);
            handles
                .into_iter()
                .map(move |handle| handle.join().unwrap())
                .collect::<Vec<_>>()
        });

        for handle in handles {
            core::iter::repeat_with(|| STRS.iter())
                .flatten()
                .zip(handle)
                .for_each(|(str, joined)| assert_eq!(*str, INTERNER.resolve(joined)));
        }

        assert_eq!(INTERNER.interned_strings(), 6);
    }
}
