use std::fmt::{Debug, Display};

#[cfg(feature = "reflect")]
use bevy_reflect::{FromReflect, Reflect, ReflectFromReflect};
#[cfg(all(feature = "reflect", feature = "serde"))]
use bevy_reflect::{ReflectDeserialize, ReflectSerialize};

use crate::interner::{Interner, RawStringId};

// `buffer` needs to be public for use in benchmarks.
pub mod buffer;
pub mod interner;
mod lazy;

pub use crate::lazy::LazyStringId;

/// The static interner used by [`StringId`].
static INTERNER: Interner = Interner::new();

/// Gets the number of bytes that have been allocated for interned strings.
pub fn allocated_bytes() -> usize {
    INTERNER.allocated_bytes()
}

/// Gets the number of bytes that can be interned without allocating.
pub fn reserved_bytes() -> usize {
    INTERNER.reserved_bytes()
}

/// Gets the number of strings that have been interned.
pub fn interned_strings() -> usize {
    INTERNER.interned_strings()
}

/// A lightweight string identifier.
///
/// All strings used to produce `StringId`s are deduplicated, and copied into a `'static` buffer.
/// The same string will produce the same `StringId` for the application lifetime, but the specific
/// values of ids may change between runs.
///
/// When the `serde` feature is enabled `StringId`s can be serialized to, or deserialized from
/// string values.
///
/// # Example
///
/// ```
/// use string_id::StringId;
///
/// let id = StringId::new("Any string");
/// assert_eq!(id, StringId::new("Any string"));
/// assert_eq!(id.resolve(), "Any string");
/// ```
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "reflect", derive(Reflect))]
#[cfg_attr(
    feature = "reflect",
    reflect(opaque, from_reflect = false, FromReflect, Debug, Clone)
)]
#[cfg_attr(
    all(feature = "reflect", feature = "serde"),
    reflect(Serialize, Deserialize)
)]
#[repr(transparent)]
pub struct StringId(RawStringId);

impl StringId {
    /// Creates a new `StringId` from the given string.
    ///
    /// Prefer to use [`StringId::new_static`] when possible, as it avoids copying the string into
    /// the static buffer.
    pub fn new(s: impl AsRef<str>) -> Self {
        Self(INTERNER.intern(s.as_ref()))
    }

    /// Creates a new `StringId` from the given `'static` string.
    ///
    /// The string will not be copied into the static buffer.
    pub fn new_static(s: &'static str) -> Self {
        Self(INTERNER.intern_static(s))
    }

    /// Retrieves the `StringId` of `s` *without* interning it.
    pub fn get(s: impl AsRef<str>) -> Option<Self> {
        INTERNER.get(s.as_ref()).map(Self)
    }

    /// Resolves the string associated with this id.
    pub fn resolve(self) -> &'static str {
        INTERNER.resolve(self.0)
    }
}

impl From<&'static str> for StringId {
    fn from(value: &'static str) -> Self {
        Self::new_static(value)
    }
}

impl From<String> for StringId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl Debug for StringId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "StringId({})", self.0.into_inner())
    }
}

impl Display for StringId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(self.resolve(), f)
    }
}

#[cfg(feature = "reflect")]
impl FromReflect for StringId {
    fn from_reflect(reflect: &dyn bevy_reflect::PartialReflect) -> Option<Self> {
        reflect.try_downcast_ref::<&str>().map(Self::new)
    }
}

#[cfg(feature = "serde")]
impl serde_core::Serialize for StringId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde_core::Serializer,
    {
        self.resolve().serialize(serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde_core::Deserialize<'de> for StringId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde_core::Deserializer<'de>,
    {
        <&str>::deserialize(deserializer).map(Self::new)
    }
}

#[cfg(test)]
mod tests {
    use rand::{RngExt, SeedableRng, distr::Alphanumeric};
    use rand_chacha::ChaCha8Rng;

    use super::*;

    #[test]
    fn same_string_interns_to_same_id() {
        assert_eq!(StringId::new_static(""), StringId::new_static(""));
        assert_eq!(StringId::new_static("Foo"), StringId::new("Foo"));
        assert_eq!(StringId::new("Bar"), StringId::new_static("Bar"));
        assert_eq!(StringId::new("Baz"), StringId::new("Baz"));

        assert_ne!(StringId::new(""), StringId::new("Foo"));
        assert_ne!(StringId::new(""), StringId::new("Bar"));
        assert_ne!(StringId::new(""), StringId::new("Baz"));

        assert_ne!(StringId::new("Foo"), StringId::new("Bar"));
        assert_ne!(StringId::new("Foo"), StringId::new("Baz"));

        assert_ne!(StringId::new("Bar"), StringId::new("Baz"));
    }

    #[test]
    fn resolves_to_same_string() {
        assert_eq!(StringId::new_static("").resolve(), "");
        assert_eq!(StringId::new_static("asdf").resolve(), "asdf");

        let mut rng = ChaCha8Rng::seed_from_u64(0xb1677be66fe11476);
        for _ in 0..10 {
            let n = rng.random_range(16..256);
            let s: String = core::iter::repeat_with(|| rng.sample(Alphanumeric) as char)
                .take(n)
                .collect();
            assert_eq!(StringId::new(&s).resolve(), &s);
        }
    }

    #[test]
    fn resolved_strings_are_static() {
        fn assert_static(_: &'static str) {}

        assert_static(StringId::new_static("Am I still 'static?").resolve());
        assert_static(StringId::new("Am I extended to 'static?").resolve());
    }
}
