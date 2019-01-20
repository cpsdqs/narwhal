use std::borrow::Borrow;
use std::cmp::Ordering;
use std::marker::PhantomData;
use std::{fmt, ops};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum SingleValue {}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum ValueSet {}

/// A map structure that stores data sorted by key as tuples in a [Vec], allowing for retrieval
/// using binary search.
///
/// A single key may be associated with multiple values if M is [ValueSet].
#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) struct BSMap<K, V, Mode = SingleValue>(Vec<(K, V)>, PhantomData<Mode>);

impl<K, V, M> BSMap<K, V, M> {
    pub fn new() -> BSMap<K, V, M> {
        BSMap(Vec::new(), PhantomData)
    }
    /*
        pub fn with_capacity(capacity: usize) -> BSMap<K, V, M> {
            BSMap(Vec::with_capacity(capacity), PhantomData)
        }

        pub fn capacity(&self) -> usize {
            self.0.capacity()
        }

        pub fn reserve(&mut self, additional: usize) {
            self.0.reserve(additional);
        }

        pub fn shrink_to_fit(&mut self) {
            self.0.shrink_to_fit();
        }
    */

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    pub fn iter(&self) -> impl Iterator<Item = &(K, V)> {
        self.0.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut (K, V)> {
        self.0.iter_mut()
    }
}

impl<K: Ord, V, M> BSMap<K, V, M> {
    pub fn contains_key<Q: ?Sized>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Ord,
    {
        self.0
            .binary_search_by_key(&key, |&(ref k, _)| k.borrow())
            .is_ok()
    }

    fn range_bounds<F: FnMut(&K) -> Ordering>(&self, mut f: F) -> (usize, usize) {
        let lower_bound = self
            .0
            .binary_search_by(|probe| match f(&probe.0) {
                Ordering::Equal => Ordering::Greater,
                x => x,
            })
            .err()
            .unwrap();
        let upper_bound = self
            .0
            .binary_search_by(|probe| match f(&probe.0) {
                Ordering::Equal => Ordering::Less,
                x => x,
            })
            .err()
            .unwrap();
        (lower_bound, upper_bound)
    }

    /// Iterates over all items in the range where the closure evaluates to [Ordering::Equal].
    pub fn range_by<F: FnMut(&K) -> Ordering>(&self, f: F) -> impl Iterator<Item = &(K, V)> {
        let (lower_bound, upper_bound) = self.range_bounds(f);
        self.0[lower_bound..upper_bound].iter()
    }

    pub fn range_by_key<'a, T: Ord, F: FnMut(&K) -> &T>(
        &'a self,
        key: T,
        mut f: F,
    ) -> impl Iterator<Item = &'a (K, V)> {
        self.range_by(move |probe| f(probe).cmp(&key))
    }

    /// Returns the key with the greatest [Ord].
    pub fn greatest_key(&self) -> Option<&K> {
        self.0.last().map(|&(ref k, _)| k)
    }
}

impl<K: Ord, V> BSMap<K, V, SingleValue> {
    pub fn get<Q: ?Sized>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Ord,
    {
        self.0
            .binary_search_by_key(&key, |&(ref k, _)| k.borrow())
            .ok()
            .map(|i| &self.0[i].1)
    }

    pub fn get_mut<Q: ?Sized>(&mut self, key: &Q) -> Option<&mut V>
    where
        K: Borrow<Q>,
        Q: Ord,
    {
        self.0
            .binary_search_by_key(&key, |&(ref k, _)| k.borrow())
            .ok()
            .map(move |i| &mut self.0[i].1)
    }

    pub fn insert(&mut self, key: K, value: V) {
        match self.0.binary_search_by_key(&&key, |&(ref k, _)| k) {
            Ok(i) => self.0[i] = (key, value),
            Err(i) => self.0.insert(i, (key, value)),
        }
    }

    pub fn remove<Q: ?Sized>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Ord,
    {
        self.0
            .binary_search_by_key(&key, |&(ref k, _)| k.borrow())
            .ok()
            .map(|i| self.0.remove(i).1)
    }
}

impl<K, V: Ord> BSMap<K, V, SingleValue> {
    /*
    pub fn get_by_value<Q: ?Sized>(&self, key: &Q) -> Option<&K>
    where
        V: Borrow<Q>,
        Q: Ord,
    {
        self.0
            .binary_search_by_key(&key, |&(_, ref v)| v.borrow())
            .ok()
            .map(|i| &self.0[i].0)
    }
    */
}

impl<K: Ord, V: Eq> BSMap<K, V, ValueSet> {
    pub fn range<'a>(&'a self, key: K) -> impl Iterator<Item = &'a (K, V)> {
        self.range_by_key(key, |probe| probe)
    }

    pub fn contains_entry(&self, key: &K, value: &V) -> bool {
        self.range_by(move |probe| probe.cmp(key))
            .find(|&(_, v)| v == value)
            .is_some()
    }

    pub fn insert_value(&mut self, key: K, value: V) {
        if self.contains_entry(&key, &value) {
            return;
        }
        let (_, upper_bound) = self.range_bounds(|probe| probe.cmp(&key));
        self.0.insert(upper_bound, (key, value));
    }

    pub fn remove_value(&mut self, key: &K, value: &V) -> Option<V> {
        let (lower_bound, _) = self.range_bounds(|probe| probe.cmp(&key));
        if let Some(index) = self
            .0
            .iter()
            .skip(lower_bound)
            .position(|&(_, ref v)| v == value)
        {
            Some(self.0.remove(index).1)
        } else {
            None
        }
    }
}

impl<'a, K: Ord, Q: ?Sized, V> ops::Index<&'a Q> for BSMap<K, V, SingleValue>
where
    K: Borrow<Q>,
    Q: Ord,
{
    type Output = V;
    fn index(&self, index: &Q) -> &V {
        self.get(index).unwrap()
    }
}

impl<'a, K: Ord, Q: ?Sized, V> ops::IndexMut<&'a Q> for BSMap<K, V, SingleValue>
where
    K: Borrow<Q>,
    Q: Ord,
{
    fn index_mut(&mut self, index: &Q) -> &mut V {
        self.get_mut(index).unwrap()
    }
}

impl<K: PartialEq + fmt::Debug, V: fmt::Debug, M> fmt::Debug for BSMap<K, V, M> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "BSMap {{ ")?;
        let mut last_key = None;
        for ((k, v), i) in self.0.iter().zip(0..) {
            if Some(k) == last_key {
                write!(f, ", {:?}", v)?;
            } else {
                if i != 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{:?} => {:?}", k, v)?;
            }
            last_key = Some(k);
        }
        if !self.is_empty() {
            write!(f, " ")?;
        }
        write!(f, "}}")
    }
}

/// An interleaved iterator meant for joining two datasets with one shared axis and different sample
/// rates.
///
/// Joins two [Iterator]s by extracting a cursor value (C) from each iterator item using Af and Bf
/// and then returning the items ordered with increasing cursor value.
pub(crate) struct Interleaved<A, B, Af, Bf, Ai, Bi, C> {
    a: A,
    b: B,
    a_index: usize,
    b_index: usize,
    af: Af,
    bf: Bf,
    a_buffer: Vec<(C, Ai)>,
    b_buffer: Vec<(C, Bi)>,
}
impl<A, B, Ai, Bi, Af, Bf, C> Interleaved<A, B, Af, Bf, Ai, Bi, C>
where
    A: Iterator<Item = Ai>,
    B: Iterator<Item = Bi>,
    Af: Fn(&Ai) -> C,
    Bf: Fn(&Bi) -> C,
    C: PartialOrd,
{
    pub(crate) fn new(a: A, b: B, af: Af, bf: Bf) -> Interleaved<A, B, Af, Bf, Ai, Bi, C> {
        Interleaved {
            a,
            b,
            a_index: 0,
            b_index: 0,
            af,
            bf,
            a_buffer: Vec::new(),
            b_buffer: Vec::new(),
        }
    }
}
pub(crate) enum InterleavedItem<Ai, Bi> {
    A(Ai, usize),
    B(Bi, usize),
}
impl<A, B, Ai, Bi, Af, Bf, C> Iterator for Interleaved<A, B, Af, Bf, Ai, Bi, C>
where
    A: Iterator<Item = Ai>,
    B: Iterator<Item = Bi>,
    Af: Fn(&Ai) -> C,
    Bf: Fn(&Bi) -> C,
    C: PartialOrd,
{
    type Item = InterleavedItem<Ai, Bi>;
    fn next(&mut self) -> Option<InterleavedItem<Ai, Bi>> {
        if self.a_buffer.is_empty() {
            if let Some(next) = self.a.next() {
                let c = (self.af)(&next);
                self.a_buffer.push((c, next));
            }
        }
        if self.b_buffer.is_empty() {
            if let Some(next) = self.b.next() {
                let c = (self.bf)(&next);
                self.b_buffer.push((c, next));
            }
        }
        match (self.a_buffer.is_empty(), self.b_buffer.is_empty()) {
            (true, true) => None,
            (true, false) => {
                self.b_index += 1;
                Some(InterleavedItem::B(
                    self.b_buffer.remove(0).1,
                    self.b_index - 1,
                ))
            }
            (false, true) => {
                self.a_index += 1;
                Some(InterleavedItem::A(
                    self.a_buffer.remove(0).1,
                    self.a_index - 1,
                ))
            }
            (false, false) => {
                if self.a_buffer[0].0 > self.b_buffer[0].0 {
                    self.b_index += 1;
                    Some(InterleavedItem::B(
                        self.b_buffer.remove(0).1,
                        self.b_index - 1,
                    ))
                } else {
                    self.a_index += 1;
                    Some(InterleavedItem::A(
                        self.a_buffer.remove(0).1,
                        self.a_index - 1,
                    ))
                }
            }
        }
    }
}
