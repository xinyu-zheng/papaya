use crate::raw::{self, InsertResult};
use crate::Equivalent;
use seize::{Collector, LocalGuard, OwnedGuard};

use crate::map::ResizeMode;
use std::collections::hash_map::RandomState;
use std::fmt;
use std::hash::{BuildHasher, Hash};
use std::marker::PhantomData;

/// A concurrent hash set.
///
/// Most hash set operations require a [`Guard`](crate::Guard), which can be acquired through
/// [`HashSet::guard`] or using the [`HashSet::pin`] API. See the [crate-level documentation](crate#usage)
/// for details.
pub struct HashSet<K, S = RandomState> {
    raw: raw::HashMap<K, (), S>,
}

// Safety: We only ever hand out &K through shared references to the map,
// so normal Send/Sync rules apply. We never expose owned or mutable references
// to keys or values.
unsafe impl<K: Send, S: Send> Send for HashSet<K, S> {}
unsafe impl<K: Sync, S: Sync> Sync for HashSet<K, S> {}

/// A builder for a [`HashSet`].
///
/// # Examples
///
/// ```rust
/// use papaya::{HashSet, ResizeMode};
/// use seize::Collector;
/// use std::collections::hash_map::RandomState;
///
/// let set: HashSet<i32> = HashSet::builder()
///     // Set the initial capacity.
///     .capacity(2048)
///     // Set the hasher.
///     .hasher(RandomState::new())
///     // Set the resize mode.
///     .resize_mode(ResizeMode::Blocking)
///     // Set a custom garbage collector.
///     .collector(Collector::new().batch_size(128))
///     // Construct the hash set.
///     .build();
/// ```
pub struct HashSetBuilder<K, S = RandomState> {
    hasher: S,
    capacity: usize,
    collector: Collector,
    resize_mode: ResizeMode,
    _kv: PhantomData<K>,
}

impl<K> HashSetBuilder<K> {
    /// Set the hash builder used to hash keys.
    ///
    /// Warning: `hash_builder` is normally randomly generated, and is designed
    /// to allow HashSets to be resistant to attacks that cause many collisions
    /// and very poor performance. Setting it manually using this function can
    /// expose a DoS attack vector.
    ///
    /// The `hash_builder` passed should implement the [`BuildHasher`] trait for
    /// the HashSet to be useful, see its documentation for details.
    pub fn hasher<S>(self, hasher: S) -> HashSetBuilder<K, S> {
        HashSetBuilder {
            hasher,
            capacity: self.capacity,
            collector: self.collector,
            resize_mode: self.resize_mode,
            _kv: PhantomData,
        }
    }
}

impl<K, S> HashSetBuilder<K, S> {
    /// Set the initial capacity of the set.
    ///
    /// The set should be able to hold at least `capacity` elements before resizing.
    /// However, the capacity is an estimate, and the set may prematurely resize due
    /// to poor hash distribution. If `capacity` is 0, the hash set will not allocate.
    pub fn capacity(self, capacity: usize) -> HashSetBuilder<K, S> {
        HashSetBuilder {
            capacity,
            hasher: self.hasher,
            collector: self.collector,
            resize_mode: self.resize_mode,
            _kv: PhantomData,
        }
    }

    /// Set the resizing mode of the set. See [`ResizeMode`] for details.
    pub fn resize_mode(self, resize_mode: ResizeMode) -> Self {
        HashSetBuilder {
            resize_mode,
            hasher: self.hasher,
            capacity: self.capacity,
            collector: self.collector,
            _kv: PhantomData,
        }
    }

    /// Set the [`seize::Collector`] used for garbage collection.
    ///
    /// This method may be useful when you want more control over garbage collection.
    ///
    /// Note that all `Guard` references used to access the set must be produced by
    /// the provided `collector`.
    pub fn collector(self, collector: Collector) -> Self {
        HashSetBuilder {
            collector,
            hasher: self.hasher,
            capacity: self.capacity,
            resize_mode: self.resize_mode,
            _kv: PhantomData,
        }
    }

    /// Construct a [`HashSet`] from the builder, using the configured options.
    pub fn build(self) -> HashSet<K, S> {
        HashSet {
            raw: raw::HashMap::new(self.capacity, self.hasher, self.resize_mode),
        }
    }
}

impl<K, S> fmt::Debug for HashSetBuilder<K, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HashSetBuilder")
            .field("capacity", &self.capacity)
            .field("collector", &self.collector)
            .field("resize_mode", &self.resize_mode)
            .finish()
    }
}

impl<K> HashSet<K> {
    /// Creates an empty `HashSet`.
    ///
    /// The hash map is initially created with a capacity of 0, so it will not allocate
    /// until it is first inserted into.
    ///
    /// # Examples
    ///
    /// ```
    /// use papaya::HashSet;
    /// let map: HashSet<&str> = HashSet::new();
    /// ```
    pub fn new() -> HashSet<K> {
        HashSet::with_capacity_and_hasher(0, RandomState::new())
    }

    /// Creates an empty `HashSet` with the specified capacity.
    ///
    /// The set should be able to hold at least `capacity` elements before resizing.
    /// However, the capacity is an estimate, and the set may prematurely resize due
    /// to poor hash distribution. If `capacity` is 0, the hash set will not allocate.
    ///
    /// # Examples
    ///
    /// ```
    /// use papaya::HashSet;
    /// let set: HashSet<&str> = HashSet::with_capacity(10);
    /// ```
    pub fn with_capacity(capacity: usize) -> HashSet<K> {
        HashSet::with_capacity_and_hasher(capacity, RandomState::new())
    }

    /// Returns a builder for a `HashSet`.
    ///
    /// The builder can be used for more complex configuration, such as using
    /// a custom [`Collector`], or [`ResizeMode`].
    pub fn builder() -> HashSetBuilder<K> {
        HashSetBuilder {
            capacity: 0,
            hasher: RandomState::default(),
            collector: Collector::new(),
            resize_mode: ResizeMode::default(),
            _kv: PhantomData,
        }
    }
}

impl<K, S> Default for HashSet<K, S>
where
    S: Default,
{
    fn default() -> Self {
        HashSet::with_hasher(S::default())
    }
}

impl<K, S> HashSet<K, S> {
    /// Creates an empty `HashSet` which will use the given hash builder to hash
    /// keys.
    ///
    /// Warning: `hash_builder` is normally randomly generated, and is designed
    /// to allow HashSets to be resistant to attacks that cause many collisions
    /// and very poor performance. Setting it manually using this function can
    /// expose a DoS attack vector.
    ///
    /// The `hash_builder` passed should implement the [`BuildHasher`] trait for
    /// the HashSet to be useful, see its documentation for details.
    ///
    /// # Examples
    ///
    /// ```
    /// use papaya::HashSet;
    /// use std::hash::RandomState;
    ///
    /// let s = RandomState::new();
    /// let set = HashSet::with_hasher(s);
    /// set.pin().insert(1);
    /// ```
    pub fn with_hasher(hash_builder: S) -> HashSet<K, S> {
        HashSet::with_capacity_and_hasher(0, hash_builder)
    }

    /// Creates an empty `HashSet` with at least the specified capacity, using
    /// `hash_builder` to hash the keys.
    ///
    /// The set should be able to hold at least `capacity` elements before resizing.
    /// However, the capacity is an estimate, and the set may prematurely resize due
    /// to poor hash distribution. If `capacity` is 0, the hash set will not allocate.
    ///
    /// Warning: `hash_builder` is normally randomly generated, and is designed
    /// to allow HashSets to be resistant to attacks that cause many collisions
    /// and very poor performance. Setting it manually using this function can
    /// expose a DoS attack vector.
    ///
    /// The `hasher` passed should implement the [`BuildHasher`] trait for
    /// the HashSet to be useful, see its documentation for details.
    ///
    /// # Examples
    ///
    /// ```
    /// use papaya::HashSet;
    /// use std::hash::RandomState;
    ///
    /// let s = RandomState::new();
    /// let set = HashSet::with_capacity_and_hasher(10, s);
    /// set.pin().insert(1);
    /// ```
    pub fn with_capacity_and_hasher(capacity: usize, hash_builder: S) -> HashSet<K, S> {
        HashSet {
            raw: raw::HashMap::new(capacity, hash_builder, ResizeMode::default()),
        }
    }

    /// Returns a pinned reference to the set.
    ///
    /// The returned reference manages a guard internally, preventing garbage collection
    /// for as long as it is held. See the [crate-level documentation](crate#usage) for details.
    #[inline]
    pub fn pin(&self) -> HashSetRef<'_, K, S> {
        HashSetRef { set: self }
    }

    /// Returns a pinned reference to the set.
    ///
    /// Unlike [`HashSet::pin`], the returned reference implements `Send` and `Sync`,
    /// allowing it to be held across `.await` points in work-stealing schedulers.
    /// This is especially useful for iterators.
    ///
    /// The returned reference manages a guard internally, preventing garbage collection
    /// for as long as it is held. See the [crate-level documentation](crate#usage) for details.
    #[inline]
    pub fn pin_owned(&self) -> HashSetRef<'_, K, S> {
        HashSetRef { set: self }
    }

    /*
    /// Returns a guard for use with this set.
    ///
    /// Note that holding on to a guard prevents garbage collection.
    /// See the [crate-level documentation](crate#usage) for details.
    #[inline]
    pub fn guard(&self) -> LocalGuard<'_> {
        self.raw.collector().enter()
    }
    */

    /*
    /// Returns an owned guard for use with this set.
    ///
    /// Owned guards implement `Send` and `Sync`, allowing them to be held across
    /// `.await` points in work-stealing schedulers. This is especially useful
    /// for iterators.
    ///
    /// Note that holding on to a guard prevents garbage collection.
    /// See the [crate-level documentation](crate#usage) for details.
    #[inline]
    pub fn owned_guard(&self) -> OwnedGuard<'_> {
        self.raw.collector().enter_owned()
    }
    */
}

impl<K, S> HashSet<K, S>
where
    K: Hash + Eq,
    S: BuildHasher,
{
    /// Returns the number of entries in the set.
    ///
    /// # Examples
    ///
    /// ```
    /// use papaya::HashSet;
    ///
    /// let set = HashSet::new();
    ///
    /// set.pin().insert(1);
    /// set.pin().insert(2);
    /// assert!(set.len() == 2);
    /// ```
    #[inline]
    pub fn len(&self) -> usize {
        self.raw.len()
    }

    /// Returns `true` if the set is empty. Otherwise returns `false`.
    ///
    /// # Examples
    ///
    /// ```
    /// use papaya::HashSet;
    ///
    /// let set = HashSet::new();
    /// assert!(set.is_empty());
    /// set.pin().insert("a");
    /// assert!(!set.is_empty());
    /// ```
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns `true` if the set contains a value for the specified key.
    ///
    /// The key may be any borrowed form of the set's key type, but
    /// [`Hash`] and [`Eq`] on the borrowed form *must* match those for
    /// the key type.
    ///
    /// [`Eq`]: std::cmp::Eq
    /// [`Hash`]: std::hash::Hash
    ///
    ///
    /// # Examples
    ///
    /// ```
    /// use papaya::HashSet;
    ///
    /// let set = HashSet::new();
    /// set.pin().insert(1);
    /// assert_eq!(set.pin().contains(&1), true);
    /// assert_eq!(set.pin().contains(&2), false);
    /// ```
    #[inline]
    pub fn contains<Q>(&self, key: &Q) -> bool
    where
        Q: Equivalent<K> + Hash + ?Sized,
    {
        self.get(key).is_some()
    }

    /// Returns a reference to the value corresponding to the key.
    ///
    /// The key may be any borrowed form of the set's key type, but
    /// [`Hash`] and [`Eq`] on the borrowed form *must* match those for
    /// the key type.
    ///
    /// [`Eq`]: std::cmp::Eq
    /// [`Hash`]: std::hash::Hash
    ///
    /// # Examples
    ///
    /// ```
    /// use papaya::HashSet;
    ///
    /// let set = HashSet::new();
    /// set.pin().insert(1);
    /// assert_eq!(set.pin().get(&1), Some(&1));
    /// assert_eq!(set.pin().get(&2), None);
    /// ```
    #[inline]
    pub fn get<'g, Q>(&self, key: &Q) -> Option<&'g K>
    where
        Q: Equivalent<K> + Hash + ?Sized,
    {
        match self.raw.get(key) {
            Some((key, _)) => Some(key),
            None => None,
        }
    }

    /// Inserts a value into the set.
    ///
    /// If the set did not have this key present, `true` is returned.
    ///
    /// If the set did have this key present, `false` is returned and the old
    /// value is not updated. This matters for types that can be `==` without
    /// being identical. See the [standard library documentation] for details.
    ///
    /// [standard library documentation]: https://doc.rust-lang.org/std/collections/index.html#insert-and-complex-keys
    ///
    /// # Examples
    ///
    /// ```
    /// use papaya::HashSet;
    ///
    /// let set = HashSet::new();
    /// assert_eq!(set.pin().insert(37), true);
    /// assert_eq!(set.pin().is_empty(), false);
    ///
    /// set.pin().insert(37);
    /// assert_eq!(set.pin().insert(37), false);
    /// assert_eq!(set.pin().get(&37), Some(&37));
    /// ```
    #[inline]
    pub fn insert(&self, key: K) -> bool {
        match self.raw.insert(key, (), true) {
            InsertResult::Inserted(_) => true,
            InsertResult::Replaced(_) => false,
            InsertResult::Error { .. } => unreachable!(),
        }
    }

    /// Removes a key from the set, returning the value at the key if the key
    /// was previously in the set.
    ///
    /// The key may be any borrowed form of the set's key type, but
    /// [`Hash`] and [`Eq`] on the borrowed form *must* match those for
    /// the key type.
    ///
    /// # Examples
    ///
    /// ```
    /// use papaya::HashSet;
    ///
    /// let set = HashSet::new();
    /// set.pin().insert(1);
    /// assert_eq!(set.pin().remove(&1), true);
    /// assert_eq!(set.pin().remove(&1), false);
    /// ```
    #[inline]
    pub fn remove<Q>(&self, key: &Q) -> bool
    where
        Q: Equivalent<K> + Hash + ?Sized,
    {
        match self.raw.remove(key) {
            Some((_, _)) => true,
            None => false,
        }
    }

    /// Tries to reserve capacity for `additional` more elements to be inserted
    /// in the `HashSet`.
    ///
    /// After calling this method, the set should be able to hold at least `capacity` elements
    /// before resizing. However, the capacity is an estimate, and the set may prematurely resize
    /// due to poor hash distribution. The collection may also reserve more space to avoid frequent
    /// reallocations.
    ///
    /// # Panics
    ///
    /// Panics if the new allocation size overflows `usize`.
    ///
    /// # Examples
    ///
    /// ```
    /// use papaya::HashSet;
    ///
    /// let set: HashSet<&str> = HashSet::new();
    /// set.pin().reserve(10);
    /// ```
    #[inline]
    pub fn reserve(&self, additional: usize) {
        self.raw.reserve(additional)
    }

    /// Clears the set, removing all values.
    ///
    /// Note that this method will block until any in-progress resizes are
    /// completed before proceeding. See the [consistency](crate#consistency)
    /// section for details.
    ///
    /// # Examples
    ///
    /// ```
    /// use papaya::HashSet;
    ///
    /// let set = HashSet::new();
    ///
    /// set.pin().insert(1);
    /// set.pin().clear();
    /// assert!(set.pin().is_empty());
    /// ```
    #[inline]
    pub fn clear(&self) {
        self.raw.clear()
    }

    /// Retains only the elements specified by the predicate.
    ///
    /// In other words, remove all values `v` for which `f(&v)` returns `false`.
    /// The elements are visited in unsorted (and unspecified) order.
    ///
    /// Note the function may be called more than once for a given key if its value is
    /// concurrently modified during removal.
    ///
    /// Additionally, this method will block until any in-progress resizes are
    /// completed before proceeding. See the [consistency](crate#consistency)
    /// section for details.
    ///
    /// # Examples
    ///
    /// ```
    /// use papaya::HashSet;
    ///
    /// let mut set: HashSet<i32> = (0..8).collect();
    /// set.pin().retain(|&v| v % 2 == 0);
    /// assert_eq!(set.len(), 4);
    /// assert_eq!(set.pin().contains(&1), false);
    /// assert_eq!(set.pin().contains(&2), true);
    /// ```
    #[inline]
    pub fn retain<F>(&mut self, mut f: F)
    where
        F: FnMut(&K) -> bool,
    {
        self.raw.retain(|k, _| f(k))
    }

    /// An iterator visiting all values in arbitrary order.
    ///
    /// Note that this method will block until any in-progress resizes are
    /// completed before proceeding. See the [consistency](crate#consistency)
    /// section for details.
    ///
    /// # Examples
    ///
    /// ```
    /// use papaya::HashSet;
    ///
    /// let set = HashSet::from([
    ///     "a",
    ///     "b",
    ///     "c"
    /// ]);
    ///
    /// for val in set.pin().iter() {
    ///     println!("val: {val}");
    /// }
    #[inline]
    pub fn iter<'g>(&self) -> Iter<'g, K> {
        Iter {
            raw: self.raw.iter(),
        }
    }
}

impl<K, S> PartialEq for HashSet<K, S>
where
    K: Hash + Eq,
    S: BuildHasher,
{
    fn eq(&self, other: &Self) -> bool {
        if self.len() != other.len() {
            return false;
        }

        let mut iter = self.iter();
        iter.all(|key| other.get(key).is_some())
    }
}

impl<K, S> Eq for HashSet<K, S>
where
    K: Hash + Eq,
    S: BuildHasher,
{
}

impl<K, S> fmt::Debug for HashSet<K, S>
where
    K: Hash + Eq + fmt::Debug,
    S: BuildHasher,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.iter()).finish()
    }
}

impl<K, S> Extend<K> for &HashSet<K, S>
where
    K: Hash + Eq,
    S: BuildHasher,
{
    fn extend<T: IntoIterator<Item = K>>(&mut self, iter: T) {
        // from `hashbrown::HashSet::extend`:
        // Keys may be already present or show multiple times in the iterator.
        // Reserve the entire hint lower bound if the set is empty.
        // Otherwise reserve half the hint (rounded up), so the set
        // will only resize twice in the worst case.
        let iter = iter.into_iter();
        let reserve = if self.is_empty() {
            iter.size_hint().0
        } else {
            (iter.size_hint().0 + 1) / 2
        };

        self.reserve(reserve);

        for key in iter {
            self.insert(key);
        }
    }
}

impl<'a, K, S> Extend<&'a K> for &HashSet<K, S>
where
    K: Copy + Hash + Eq + 'a,
    S: BuildHasher,
{
    fn extend<T: IntoIterator<Item = &'a K>>(&mut self, iter: T) {
        self.extend(iter.into_iter().copied());
    }
}

impl<K, const N: usize> From<[K; N]> for HashSet<K, RandomState>
where
    K: Hash + Eq,
{
    fn from(arr: [K; N]) -> Self {
        HashSet::from_iter(arr)
    }
}

impl<K, S> FromIterator<K> for HashSet<K, S>
where
    K: Hash + Eq,
    S: BuildHasher + Default,
{
    fn from_iter<T: IntoIterator<Item = K>>(iter: T) -> Self {
        let mut iter = iter.into_iter();

        if let Some(key) = iter.next() {
            let (lower, _) = iter.size_hint();
            let set = HashSet::with_capacity_and_hasher(lower.saturating_add(1), S::default());

            // Ideally we could use an unprotected guard here. However, `insert`
            // returns references to values that were replaced and retired, so
            // we need a "real" guard. A `raw_insert` method that strictly returns
            // pointers would fix this.
            {
                let set = set.pin();
                set.insert(key);
                for key in iter {
                    set.insert(key);
                }
            }

            set
        } else {
            Self::default()
        }
    }
}

impl<K, S> Clone for HashSet<K, S>
where
    K: Clone + Hash + Eq,
    S: BuildHasher + Clone,
{
    fn clone(&self) -> HashSet<K, S> {
        let other = HashSet::builder()
            .capacity(self.len())
            .hasher(self.raw.hasher.clone())
            .collector(seize::Collector::new())
            .build();

        {
            for key in self.iter() {
                other.insert(key.clone());
            }
        }

        other
    }
}

/// A pinned reference to a [`HashSet`].
///
/// This type is created with [`HashSet::pin`] and can be used to easily access a [`HashSet`]
/// without explicitly managing a guard. See the [crate-level documentation](crate#usage) for details.
pub struct HashSetRef<'set, K, S> {
    set: &'set HashSet<K, S>,
}

impl<'set, K, S> HashSetRef<'set, K, S>
where
    K: Hash + Eq,
    S: BuildHasher,
{
    /// Returns a reference to the inner [`HashSet`].
    #[inline]
    pub fn set(&self) -> &'set HashSet<K, S> {
        self.set
    }

    /// Returns the number of entries in the set.
    ///
    /// See [`HashSet::len`] for details.
    #[inline]
    pub fn len(&self) -> usize {
        self.set.raw.len()
    }

    /// Returns `true` if the set is empty. Otherwise returns `false`.
    ///
    /// See [`HashSet::is_empty`] for details.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns `true` if the set contains a value for the specified key.
    ///
    /// See [`HashSet::contains`] for details.
    #[inline]
    pub fn contains<Q>(&self, key: &Q) -> bool
    where
        Q: Equivalent<K> + Hash + ?Sized,
    {
        self.get(key).is_some()
    }

    /// Returns a reference to the value corresponding to the key.
    ///
    /// See [`HashSet::get`] for details.
    #[inline]
    pub fn get<Q>(&self, key: &Q) -> Option<&K>
    where
        Q: Equivalent<K> + Hash + ?Sized,
    {
        match self.set.raw.get(key) {
            Some((k, _)) => Some(k),
            None => None,
        }
    }

    /// Inserts a key-value pair into the set.
    ///
    /// See [`HashSet::insert`] for details.
    #[inline]
    pub fn insert(&self, key: K) -> bool {
        match self.set.raw.insert(key, (), true) {
            InsertResult::Inserted(_) => true,
            InsertResult::Replaced(_) => false,
            InsertResult::Error { .. } => unreachable!(),
        }
    }

    /// Removes a key from the set, returning the value at the key if the key
    /// was previously in the set.
    ///
    /// See [`HashSet::remove`] for details.
    #[inline]
    pub fn remove<Q>(&self, key: &Q) -> bool
    where
        Q: Equivalent<K> + Hash + ?Sized,
    {
        match self.set.raw.remove(key) {
            Some((_, _)) => true,
            None => false,
        }
    }

    /// Clears the set, removing all values.
    ///
    /// See [`HashSet::clear`] for details.
    #[inline]
    pub fn clear(&self) {
        self.set.raw.clear()
    }

    /// Retains only the elements specified by the predicate.
    ///
    /// See [`HashSet::retain`] for details.
    #[inline]
    pub fn retain<F>(&mut self, mut f: F)
    where
        F: FnMut(&K) -> bool,
    {
        self.set.raw.retain(|k, _| f(k))
    }

    /// Tries to reserve capacity for `additional` more elements to be inserted
    /// in the set.
    ///
    /// See [`HashSet::reserve`] for details.
    #[inline]
    pub fn reserve(&self, additional: usize) {
        self.set.raw.reserve(additional)
    }

    /// An iterator visiting all values in arbitrary order.
    /// The iterator element type is `(&K, &V)`.
    ///
    /// See [`HashSet::iter`] for details.
    #[inline]
    pub fn iter(&self) -> Iter<'_, K> {
        Iter {
            raw: self.set.raw.iter(),
        }
    }
}

impl<K, S> fmt::Debug for HashSetRef<'_, K, S>
where
    K: Hash + Eq + fmt::Debug,
    S: BuildHasher,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.iter()).finish()
    }
}

impl<'a, K, S> IntoIterator for &'a HashSetRef<'_, K, S>
where
    K: Hash + Eq,
    S: BuildHasher,
{
    type Item = &'a K;
    type IntoIter = Iter<'a, K>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// An iterator over a set's entries.
///
/// This struct is created by the [`iter`](HashSet::iter) method on [`HashSet`]. See its documentation for details.
pub struct Iter<'g, K> {
    raw: raw::Iter<'g, K, ()>,
}

impl<'g, K: 'g> Iterator for Iter<'g, K> {
    type Item = &'g K;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.raw.next().map(|(k, _)| k)
    }
}

impl<K> fmt::Debug for Iter<'_, K>
where
    K: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list()
            .entries(Iter {
                raw: self.raw.clone(),
            })
            .finish()
    }
}
