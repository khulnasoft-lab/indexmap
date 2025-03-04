//! Opt-in access to the experimental raw entry API.
//!
//! This module is designed to mimic the raw entry API of [`HashMap`][std::collections::hash_map],
//! matching its unstable state as of Rust 1.75. See the tracking issue
//! [rust#56167](https://github.com/rust-lang/rust/issues/56167) for more details.
//!
//! The trait [`RawEntryApiV1`] and the `_v1` suffix on its methods are meant to insulate this for
//! the future, in case later breaking changes are needed. If the standard library stabilizes its
//! `hash_raw_entry` feature (or some replacement), matching *inherent* methods will be added to
//! `IndexMap` without such an opt-in trait.

use super::raw::RawTableEntry;
use super::{get_hash, IndexMapCore};
use crate::{Equivalent, HashValue, IndexMap};
use core::fmt;
use core::hash::{BuildHasher, Hash, Hasher};
use core::marker::PhantomData;
use core::mem;

/// Opt-in access to the experimental raw entry API.
///
/// See the [`raw_entry_v1`][self] module documentation for more information.
pub trait RawEntryApiV1<K, V, S>: private::Sealed {
    /// Creates a raw immutable entry builder for the [`IndexMap`].
    ///
    /// Raw entries provide the lowest level of control for searching and
    /// manipulating a map. They must be manually initialized with a hash and
    /// then manually searched.
    ///
    /// This is useful for
    /// * Hash memoization
    /// * Using a search key that doesn't work with the [`Equivalent`] trait
    /// * Using custom comparison logic without newtype wrappers
    ///
    /// Unless you are in such a situation, higher-level and more foolproof APIs like
    /// [`get`][IndexMap::get] should be preferred.
    ///
    /// Immutable raw entries have very limited use; you might instead want
    /// [`raw_entry_mut_v1`][Self::raw_entry_mut_v1].
    ///
    /// # Examples
    ///
    /// ```
    /// use core::hash::{BuildHasher, Hash};
    /// use indexmap::map::{IndexMap, RawEntryApiV1};
    ///
    /// let mut map = IndexMap::new();
    /// map.extend([("a", 100), ("b", 200), ("c", 300)]);
    ///
    /// fn compute_hash<K: Hash + ?Sized, S: BuildHasher>(hash_builder: &S, key: &K) -> u64 {
    ///     use core::hash::Hasher;
    ///     let mut state = hash_builder.build_hasher();
    ///     key.hash(&mut state);
    ///     state.finish()
    /// }
    ///
    /// for k in ["a", "b", "c", "d", "e", "f"] {
    ///     let hash = compute_hash(map.hasher(), k);
    ///     let v = map.get(k).cloned();
    ///     let kv = v.as_ref().map(|v| (&k, v));
    ///
    ///     println!("Key: {} and value: {:?}", k, v);
    ///
    ///     assert_eq!(map.raw_entry_v1().from_key(k), kv);
    ///     assert_eq!(map.raw_entry_v1().from_hash(hash, |q| *q == k), kv);
    ///     assert_eq!(map.raw_entry_v1().from_key_hashed_nocheck(hash, k), kv);
    /// }
    /// ```
    fn raw_entry_v1(&self) -> RawEntryBuilder<'_, K, V, S>;

    /// Creates a raw entry builder for the [`IndexMap`].
    ///
    /// Raw entries provide the lowest level of control for searching and
    /// manipulating a map. They must be manually initialized with a hash and
    /// then manually searched. After this, insertions into a vacant entry
    /// still require an owned key to be provided.
    ///
    /// Raw entries are useful for such exotic situations as:
    ///
    /// * Hash memoization
    /// * Deferring the creation of an owned key until it is known to be required
    /// * Using a search key that doesn't work with the [`Equivalent`] trait
    /// * Using custom comparison logic without newtype wrappers
    ///
    /// Because raw entries provide much more low-level control, it's much easier
    /// to put the `IndexMap` into an inconsistent state which, while memory-safe,
    /// will cause the map to produce seemingly random results. Higher-level and more
    /// foolproof APIs like [`entry`][IndexMap::entry] should be preferred when possible.
    ///
    /// Raw entries give mutable access to the keys. This must not be used
    /// to modify how the key would compare or hash, as the map will not re-evaluate
    /// where the key should go, meaning the keys may become "lost" if their
    /// location does not reflect their state. For instance, if you change a key
    /// so that the map now contains keys which compare equal, search may start
    /// acting erratically, with two keys randomly masking each other. Implementations
    /// are free to assume this doesn't happen (within the limits of memory-safety).
    ///
    /// # Examples
    ///
    /// ```
    /// use core::hash::{BuildHasher, Hash};
    /// use indexmap::map::{IndexMap, RawEntryApiV1};
    /// use indexmap::map::raw_entry_v1::RawEntryMut;
    ///
    /// let mut map = IndexMap::new();
    /// map.extend([("a", 100), ("b", 200), ("c", 300)]);
    ///
    /// fn compute_hash<K: Hash + ?Sized, S: BuildHasher>(hash_builder: &S, key: &K) -> u64 {
    ///     use core::hash::Hasher;
    ///     let mut state = hash_builder.build_hasher();
    ///     key.hash(&mut state);
    ///     state.finish()
    /// }
    ///
    /// // Existing key (insert and update)
    /// match map.raw_entry_mut_v1().from_key("a") {
    ///     RawEntryMut::Vacant(_) => unreachable!(),
    ///     RawEntryMut::Occupied(mut view) => {
    ///         assert_eq!(view.get(), &100);
    ///         let v = view.get_mut();
    ///         let new_v = (*v) * 10;
    ///         *v = new_v;
    ///         assert_eq!(view.insert(1111), 1000);
    ///     }
    /// }
    ///
    /// assert_eq!(map["a"], 1111);
    /// assert_eq!(map.len(), 3);
    ///
    /// // Existing key (take)
    /// let hash = compute_hash(map.hasher(), "c");
    /// match map.raw_entry_mut_v1().from_key_hashed_nocheck(hash, "c") {
    ///     RawEntryMut::Vacant(_) => unreachable!(),
    ///     RawEntryMut::Occupied(view) => {
    ///         assert_eq!(view.shift_remove_entry(), ("c", 300));
    ///     }
    /// }
    /// assert_eq!(map.raw_entry_v1().from_key("c"), None);
    /// assert_eq!(map.len(), 2);
    ///
    /// // Nonexistent key (insert and update)
    /// let key = "d";
    /// let hash = compute_hash(map.hasher(), key);
    /// match map.raw_entry_mut_v1().from_hash(hash, |q| *q == key) {
    ///     RawEntryMut::Occupied(_) => unreachable!(),
    ///     RawEntryMut::Vacant(view) => {
    ///         let (k, value) = view.insert("d", 4000);
    ///         assert_eq!((*k, *value), ("d", 4000));
    ///         *value = 40000;
    ///     }
    /// }
    /// assert_eq!(map["d"], 40000);
    /// assert_eq!(map.len(), 3);
    ///
    /// match map.raw_entry_mut_v1().from_hash(hash, |q| *q == key) {
    ///     RawEntryMut::Vacant(_) => unreachable!(),
    ///     RawEntryMut::Occupied(view) => {
    ///         assert_eq!(view.swap_remove_entry(), ("d", 40000));
    ///     }
    /// }
    /// assert_eq!(map.get("d"), None);
    /// assert_eq!(map.len(), 2);
    /// ```
    fn raw_entry_mut_v1(&mut self) -> RawEntryBuilderMut<'_, K, V, S>;
}

impl<K, V, S> RawEntryApiV1<K, V, S> for IndexMap<K, V, S> {
    fn raw_entry_v1(&self) -> RawEntryBuilder<'_, K, V, S> {
        RawEntryBuilder { map: self }
    }

    fn raw_entry_mut_v1(&mut self) -> RawEntryBuilderMut<'_, K, V, S> {
        RawEntryBuilderMut { map: self }
    }
}

/// A builder for computing where in an [`IndexMap`] a key-value pair would be stored.
///
/// This `struct` is created by the [`IndexMap::raw_entry_v1`] method, provided by the
/// [`RawEntryApiV1`] trait. See its documentation for more.
pub struct RawEntryBuilder<'a, K, V, S> {
    map: &'a IndexMap<K, V, S>,
}

impl<K, V, S> fmt::Debug for RawEntryBuilder<'_, K, V, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RawEntryBuilder").finish_non_exhaustive()
    }
}

impl<'a, K, V, S> RawEntryBuilder<'a, K, V, S> {
    /// Access an entry by key.
    pub fn from_key<Q: ?Sized>(self, key: &Q) -> Option<(&'a K, &'a V)>
    where
        S: BuildHasher,
        Q: Hash + Equivalent<K>,
    {
        self.map.get_key_value(key)
    }

    /// Access an entry by a key and its hash.
    pub fn from_key_hashed_nocheck<Q: ?Sized>(self, hash: u64, key: &Q) -> Option<(&'a K, &'a V)>
    where
        Q: Equivalent<K>,
    {
        let hash = HashValue(hash as usize);
        let i = self.map.core.get_index_of(hash, key)?;
        Some(self.map.core.entries[i].refs())
    }

    /// Access an entry by hash.
    pub fn from_hash<F>(self, hash: u64, mut is_match: F) -> Option<(&'a K, &'a V)>
    where
        F: FnMut(&K) -> bool,
    {
        let hash = HashValue(hash as usize);
        let entries = &*self.map.core.entries;
        let eq = move |&i: &usize| is_match(&entries[i].key);
        let i = *self.map.core.indices.get(hash.get(), eq)?;
        Some(entries[i].refs())
    }
}

/// A builder for computing where in an [`IndexMap`] a key-value pair would be stored.
///
/// This `struct` is created by the [`IndexMap::raw_entry_mut_v1`] method, provided by the
/// [`RawEntryApiV1`] trait. See its documentation for more.
pub struct RawEntryBuilderMut<'a, K, V, S> {
    map: &'a mut IndexMap<K, V, S>,
}

impl<K, V, S> fmt::Debug for RawEntryBuilderMut<'_, K, V, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RawEntryBuilderMut").finish_non_exhaustive()
    }
}

impl<'a, K, V, S> RawEntryBuilderMut<'a, K, V, S> {
    /// Access an entry by key.
    pub fn from_key<Q: ?Sized>(self, key: &Q) -> RawEntryMut<'a, K, V, S>
    where
        S: BuildHasher,
        Q: Hash + Equivalent<K>,
    {
        let hash = self.map.hash(key);
        self.from_key_hashed_nocheck(hash.get(), key)
    }

    /// Access an entry by a key and its hash.
    pub fn from_key_hashed_nocheck<Q: ?Sized>(self, hash: u64, key: &Q) -> RawEntryMut<'a, K, V, S>
    where
        Q: Equivalent<K>,
    {
        self.from_hash(hash, |k| Q::equivalent(key, k))
    }

    /// Access an entry by hash.
    pub fn from_hash<F>(self, hash: u64, is_match: F) -> RawEntryMut<'a, K, V, S>
    where
        F: FnMut(&K) -> bool,
    {
        let hash = HashValue(hash as usize);
        match self.map.core.raw_entry(hash, is_match) {
            Ok(raw) => RawEntryMut::Occupied(RawOccupiedEntryMut {
                raw,
                hash_builder: PhantomData,
            }),
            Err(map) => RawEntryMut::Vacant(RawVacantEntryMut {
                map,
                hash_builder: &self.map.hash_builder,
            }),
        }
    }
}

/// Raw entry for an existing key-value pair or a vacant location to
/// insert one.
pub enum RawEntryMut<'a, K, V, S> {
    /// Existing slot with equivalent key.
    Occupied(RawOccupiedEntryMut<'a, K, V, S>),
    /// Vacant slot (no equivalent key in the map).
    Vacant(RawVacantEntryMut<'a, K, V, S>),
}

impl<K: fmt::Debug, V: fmt::Debug, S> fmt::Debug for RawEntryMut<'_, K, V, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut tuple = f.debug_tuple("RawEntryMut");
        match self {
            Self::Vacant(v) => tuple.field(v),
            Self::Occupied(o) => tuple.field(o),
        };
        tuple.finish()
    }
}

impl<'a, K, V, S> RawEntryMut<'a, K, V, S> {
    /// Inserts the given default key and value in the entry if it is vacant and returns mutable
    /// references to them. Otherwise mutable references to an already existent pair are returned.
    pub fn or_insert(self, default_key: K, default_value: V) -> (&'a mut K, &'a mut V)
    where
        K: Hash,
        S: BuildHasher,
    {
        match self {
            Self::Occupied(entry) => entry.into_key_value_mut(),
            Self::Vacant(entry) => entry.insert(default_key, default_value),
        }
    }

    /// Inserts the result of the `call` function in the entry if it is vacant and returns mutable
    /// references to them. Otherwise mutable references to an already existent pair are returned.
    pub fn or_insert_with<F>(self, call: F) -> (&'a mut K, &'a mut V)
    where
        F: FnOnce() -> (K, V),
        K: Hash,
        S: BuildHasher,
    {
        match self {
            Self::Occupied(entry) => entry.into_key_value_mut(),
            Self::Vacant(entry) => {
                let (key, value) = call();
                entry.insert(key, value)
            }
        }
    }

    /// Modifies the entry if it is occupied.
    pub fn and_modify<F>(mut self, f: F) -> Self
    where
        F: FnOnce(&mut K, &mut V),
    {
        if let Self::Occupied(entry) = &mut self {
            let (k, v) = entry.get_key_value_mut();
            f(k, v);
        }
        self
    }
}

/// A raw view into an occupied entry in an [`IndexMap`].
/// It is part of the [`RawEntryMut`] enum.
pub struct RawOccupiedEntryMut<'a, K, V, S> {
    raw: RawTableEntry<'a, K, V>,
    hash_builder: PhantomData<&'a S>,
}

impl<K: fmt::Debug, V: fmt::Debug, S> fmt::Debug for RawOccupiedEntryMut<'_, K, V, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RawOccupiedEntryMut")
            .field("key", self.key())
            .field("value", self.get())
            .finish_non_exhaustive()
    }
}

impl<'a, K, V, S> RawOccupiedEntryMut<'a, K, V, S> {
    /// Return the index of the key-value pair
    #[inline]
    pub fn index(&self) -> usize {
        self.raw.index()
    }

    /// Gets a reference to the entry's key in the map.
    ///
    /// Note that this is not the key that was used to find the entry. There may be an observable
    /// difference if the key type has any distinguishing features outside of `Hash` and `Eq`, like
    /// extra fields or the memory address of an allocation.
    pub fn key(&self) -> &K {
        &self.raw.bucket().key
    }

    /// Gets a mutable reference to the entry's key in the map.
    ///
    /// Note that this is not the key that was used to find the entry. There may be an observable
    /// difference if the key type has any distinguishing features outside of `Hash` and `Eq`, like
    /// extra fields or the memory address of an allocation.
    pub fn key_mut(&mut self) -> &mut K {
        &mut self.raw.bucket_mut().key
    }

    /// Converts into a mutable reference to the entry's key in the map,
    /// with a lifetime bound to the map itself.
    ///
    /// Note that this is not the key that was used to find the entry. There may be an observable
    /// difference if the key type has any distinguishing features outside of `Hash` and `Eq`, like
    /// extra fields or the memory address of an allocation.
    pub fn into_key(self) -> &'a mut K {
        &mut self.raw.into_bucket().key
    }

    /// Gets a reference to the entry's value in the map.
    pub fn get(&self) -> &V {
        &self.raw.bucket().value
    }

    /// Gets a mutable reference to the entry's value in the map.
    ///
    /// If you need a reference which may outlive the destruction of the
    /// [`RawEntryMut`] value, see [`into_mut`][Self::into_mut].
    pub fn get_mut(&mut self) -> &mut V {
        &mut self.raw.bucket_mut().value
    }

    /// Converts into a mutable reference to the entry's value in the map,
    /// with a lifetime bound to the map itself.
    pub fn into_mut(self) -> &'a mut V {
        &mut self.raw.into_bucket().value
    }

    /// Gets a reference to the entry's key and value in the map.
    pub fn get_key_value(&self) -> (&K, &V) {
        self.raw.bucket().refs()
    }

    /// Gets a reference to the entry's key and value in the map.
    pub fn get_key_value_mut(&mut self) -> (&mut K, &mut V) {
        self.raw.bucket_mut().muts()
    }

    /// Converts into a mutable reference to the entry's key and value in the map,
    /// with a lifetime bound to the map itself.
    pub fn into_key_value_mut(self) -> (&'a mut K, &'a mut V) {
        self.raw.into_bucket().muts()
    }

    /// Sets the value of the entry, and returns the entry's old value.
    pub fn insert(&mut self, value: V) -> V {
        mem::replace(self.get_mut(), value)
    }

    /// Sets the key of the entry, and returns the entry's old key.
    pub fn insert_key(&mut self, key: K) -> K {
        mem::replace(self.key_mut(), key)
    }

    /// Remove the key, value pair stored in the map for this entry, and return the value.
    ///
    /// **NOTE:** This is equivalent to [`.swap_remove()`][Self::swap_remove], replacing this
    /// entry's position with the last element, and it is deprecated in favor of calling that
    /// explicitly. If you need to preserve the relative order of the keys in the map, use
    /// [`.shift_remove()`][Self::shift_remove] instead.
    #[deprecated(note = "`remove` disrupts the map order -- \
        use `swap_remove` or `shift_remove` for explicit behavior.")]
    pub fn remove(self) -> V {
        self.swap_remove()
    }

    /// Remove the key, value pair stored in the map for this entry, and return the value.
    ///
    /// Like [`Vec::swap_remove`][crate::Vec::swap_remove], the pair is removed by swapping it with
    /// the last element of the map and popping it off.
    /// **This perturbs the position of what used to be the last element!**
    ///
    /// Computes in **O(1)** time (average).
    pub fn swap_remove(self) -> V {
        self.swap_remove_entry().1
    }

    /// Remove the key, value pair stored in the map for this entry, and return the value.
    ///
    /// Like [`Vec::remove`][crate::Vec::remove], the pair is removed by shifting all of the
    /// elements that follow it, preserving their relative order.
    /// **This perturbs the index of all of those elements!**
    ///
    /// Computes in **O(n)** time (average).
    pub fn shift_remove(self) -> V {
        self.shift_remove_entry().1
    }

    /// Remove and return the key, value pair stored in the map for this entry
    ///
    /// **NOTE:** This is equivalent to [`.swap_remove_entry()`][Self::swap_remove_entry],
    /// replacing this entry's position with the last element, and it is deprecated in favor of
    /// calling that explicitly. If you need to preserve the relative order of the keys in the map,
    /// use [`.shift_remove_entry()`][Self::shift_remove_entry] instead.
    #[deprecated(note = "`remove_entry` disrupts the map order -- \
        use `swap_remove_entry` or `shift_remove_entry` for explicit behavior.")]
    pub fn remove_entry(self) -> (K, V) {
        self.swap_remove_entry()
    }

    /// Remove and return the key, value pair stored in the map for this entry
    ///
    /// Like [`Vec::swap_remove`][crate::Vec::swap_remove], the pair is removed by swapping it with
    /// the last element of the map and popping it off.
    /// **This perturbs the position of what used to be the last element!**
    ///
    /// Computes in **O(1)** time (average).
    pub fn swap_remove_entry(self) -> (K, V) {
        let (map, index) = self.raw.remove_index();
        map.swap_remove_finish(index)
    }

    /// Remove and return the key, value pair stored in the map for this entry
    ///
    /// Like [`Vec::remove`][crate::Vec::remove], the pair is removed by shifting all of the
    /// elements that follow it, preserving their relative order.
    /// **This perturbs the index of all of those elements!**
    ///
    /// Computes in **O(n)** time (average).
    pub fn shift_remove_entry(self) -> (K, V) {
        let (map, index) = self.raw.remove_index();
        map.shift_remove_finish(index)
    }
}

/// A view into a vacant raw entry in an [`IndexMap`].
/// It is part of the [`RawEntryMut`] enum.
pub struct RawVacantEntryMut<'a, K, V, S> {
    map: &'a mut IndexMapCore<K, V>,
    hash_builder: &'a S,
}

impl<K, V, S> fmt::Debug for RawVacantEntryMut<'_, K, V, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RawVacantEntryMut").finish_non_exhaustive()
    }
}

impl<'a, K, V, S> RawVacantEntryMut<'a, K, V, S> {
    /// Return the index where a key-value pair may be inserted.
    pub fn index(&self) -> usize {
        self.map.indices.len()
    }

    /// Inserts the given key and value into the map,
    /// and returns mutable references to them.
    pub fn insert(self, key: K, value: V) -> (&'a mut K, &'a mut V)
    where
        K: Hash,
        S: BuildHasher,
    {
        let mut h = self.hash_builder.build_hasher();
        key.hash(&mut h);
        self.insert_hashed_nocheck(h.finish(), key, value)
    }

    /// Inserts the given key and value into the map with the provided hash,
    /// and returns mutable references to them.
    pub fn insert_hashed_nocheck(self, hash: u64, key: K, value: V) -> (&'a mut K, &'a mut V) {
        let i = self.index();
        let map = self.map;
        let hash = HashValue(hash as usize);
        map.indices.insert(hash.get(), i, get_hash(&map.entries));
        debug_assert_eq!(i, map.entries.len());
        map.push_entry(hash, key, value);
        map.entries[i].muts()
    }
}

mod private {
    pub trait Sealed {}

    impl<K, V, S> Sealed for super::IndexMap<K, V, S> {}
}
