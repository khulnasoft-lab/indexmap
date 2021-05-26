use super::{Bucket, Entries, IndexSet, Iter};
use crate::util::simplify_range;

use core::cmp::Ordering;
use core::fmt;
use core::hash::{Hash, Hasher};
use core::ops::{self, Bound, Index};

/// A dynamically-sized slice of values in an `IndexSet`.
///
/// This supports indexed operations much like a `[T]` slice,
/// but not any hashed operations on the values.
///
/// Unlike `IndexSet`, `Slice` does consider the order for `PartialEq`
/// and `Eq`, and it also implements `PartialOrd`, `Ord`, and `Hash`.
#[repr(transparent)]
pub struct Slice<T> {
    pub(crate) entries: [Bucket<T>],
}

#[allow(unsafe_code)]
impl<T> Slice<T> {
    fn from_slice(entries: &[Bucket<T>]) -> &Self {
        // SAFETY: `Slice<T>` is a transparent wrapper around `[Bucket<T>]`,
        // and the lifetimes are bound together by this function's signature.
        unsafe { &*(entries as *const [Bucket<T>] as *const Self) }
    }
}

impl<T, S> IndexSet<T, S> {
    /// Returns a slice of all the values in the set.
    pub fn as_slice(&self) -> &Slice<T> {
        Slice::from_slice(self.as_entries())
    }
}

impl<'a, T> Iter<'a, T> {
    /// Returns a slice of the remaining entries in the iterator.
    pub fn as_slice(&self) -> &'a Slice<T> {
        Slice::from_slice(self.iter.as_slice())
    }
}

impl<T> Slice<T> {
    /// Return the number of elements in the set slice.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns true if the set slice contains no elements.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Get a value by index.
    ///
    /// Valid indices are *0 <= index < self.len()*
    pub fn get_index(&self, index: usize) -> Option<&T> {
        self.entries.get(index).map(Bucket::key_ref)
    }

    /// Get the first value.
    pub fn first(&self) -> Option<&T> {
        self.entries.first().map(Bucket::key_ref)
    }

    /// Get the last value.
    pub fn last(&self) -> Option<&T> {
        self.entries.last().map(Bucket::key_ref)
    }

    /// Divides one slice into two at an index.
    ///
    /// ***Panics*** if `index > len`.
    pub fn split_at(&self, index: usize) -> (&Self, &Self) {
        let (first, second) = self.entries.split_at(index);
        (Self::from_slice(first), Self::from_slice(second))
    }

    /// Returns the first value and the rest of the slice,
    /// or `None` if it is empty.
    pub fn split_first(&self) -> Option<(&T, &Self)> {
        if let Some((first, rest)) = self.entries.split_first() {
            Some((&first.key, Self::from_slice(rest)))
        } else {
            None
        }
    }

    /// Returns the last value and the rest of the slice,
    /// or `None` if it is empty.
    pub fn split_last(&self) -> Option<(&T, &Self)> {
        if let Some((last, rest)) = self.entries.split_last() {
            Some((&last.key, Self::from_slice(rest)))
        } else {
            None
        }
    }

    /// Return an iterator over the values of the set slice.
    pub fn iter(&self) -> Iter<'_, T> {
        Iter {
            iter: self.entries.iter(),
        }
    }
}

impl<'a, T> IntoIterator for &'a Slice<T> {
    type IntoIter = Iter<'a, T>;
    type Item = &'a T;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<T> Default for &'_ Slice<T> {
    fn default() -> Self {
        Slice::from_slice(&[])
    }
}

impl<T: fmt::Debug> fmt::Debug for Slice<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self).finish()
    }
}

impl<T: PartialEq> PartialEq for Slice<T> {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter().eq(other)
    }
}

impl<T: Eq> Eq for Slice<T> {}

impl<T: PartialOrd> PartialOrd for Slice<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.iter().partial_cmp(other)
    }
}

impl<T: Ord> Ord for Slice<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.iter().cmp(other)
    }
}

impl<T: Hash> Hash for Slice<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.len().hash(state);
        for value in self {
            value.hash(state);
        }
    }
}

impl<T> Index<usize> for Slice<T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        &self.entries[index].key
    }
}

// We can't have `impl<I: RangeBounds<usize>> Index<I>` because that conflicts with `Index<usize>`.
// Instead, we repeat the implementations for all the core range types.
macro_rules! impl_index {
    ($($range:ty),*) => {$(
        impl<T, S> Index<$range> for IndexSet<T, S> {
            type Output = Slice<T>;

            fn index(&self, range: $range) -> &Self::Output {
                Slice::from_slice(&self.as_entries()[range])
            }
        }

        impl<T> Index<$range> for Slice<T> {
            type Output = Self;

            fn index(&self, range: $range) -> &Self::Output {
                Slice::from_slice(&self.entries[range])
            }
        }
    )*}
}
impl_index!(
    ops::Range<usize>,
    ops::RangeFrom<usize>,
    ops::RangeFull,
    ops::RangeInclusive<usize>,
    ops::RangeTo<usize>,
    ops::RangeToInclusive<usize>
);

// NB: with MSRV 1.53, we can forward `Bound` pairs to direct slice indexing like other ranges

impl<T, S> Index<(Bound<usize>, Bound<usize>)> for IndexSet<T, S> {
    type Output = Slice<T>;

    fn index(&self, range: (Bound<usize>, Bound<usize>)) -> &Self::Output {
        let entries = self.as_entries();
        let range = simplify_range(range, entries.len());
        Slice::from_slice(&entries[range])
    }
}

impl<T> Index<(Bound<usize>, Bound<usize>)> for Slice<T> {
    type Output = Self;

    fn index(&self, range: (Bound<usize>, Bound<usize>)) -> &Self::Output {
        let entries = &self.entries;
        let range = simplify_range(range, entries.len());
        Slice::from_slice(&entries[range])
    }
}
