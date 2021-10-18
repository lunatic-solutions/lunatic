/*!
Async concurrent hashmap built on top of [dashmap](https://docs.rs/dashmap/). The majority of the
implementation was taken from [AsyncMap](https://github.com/withoutboats/waitmap), but reduced to
only export one method that is needed for .

It allows you to wait on a key until someone puts something under this key into the map.
*/

use std::borrow::Borrow;
use std::collections::hash_map::RandomState;
use std::future::Future;
use std::hash::{BuildHasher, Hash};
use std::mem;
use std::pin::Pin;
use std::task::Waker;
use std::task::{Context, Poll};

use dashmap::mapref::entry::Entry::{self, *};
use dashmap::DashMap;
use smallvec::SmallVec;

use WaitEntry::*;

/// An asynchronous concurrent hashmap.
pub struct AsyncMap<K, V, S = RandomState> {
    map: DashMap<K, WaitEntry<V>, S>,
}

impl<K: Hash + Eq, V> AsyncMap<K, V> {
    /// Make a new `AsyncMap` using the default hasher.
    pub fn new() -> AsyncMap<K, V> {
        AsyncMap {
            map: DashMap::with_hasher(RandomState::default()),
        }
    }
}

impl<K: Hash + Eq, V, S: BuildHasher + Clone> AsyncMap<K, V, S> {
    /// Inserts a key-value pair into the map.
    ///
    /// If the map did not have this key present, `None` is returned.
    ///
    /// If there are any pending `remove` calls for this key, they are woken up.
    ///
    /// If the map did have this key present, the value is updated and the old value is returned.
    pub fn insert(&self, key: K, value: V) -> Option<V> {
        match self.map.entry(key) {
            Occupied(mut entry) => {
                match mem::replace(entry.get_mut(), Filled(value)) {
                    Waiting(wakers) => {
                        drop(entry); // drop early to release lock before waking other tasks
                        wakers.wake();
                        None
                    }
                    Filled(value) => Some(value),
                }
            }
            Vacant(slot) => {
                slot.insert(Filled(value));
                None
            }
        }
    }

    pub fn wait_remove<'a: 'f, 'b: 'f, 'f, Q: ?Sized + Hash + Eq>(
        &'a self,
        qey: &'b Q,
    ) -> impl Future<Output = Option<V>> + 'f
    where
        K: Borrow<Q> + From<&'b Q>,
    {
        let key = K::from(qey);
        self.map.entry(key).or_insert(Waiting(WakerSet::new()));
        Wait::new(&self.map, qey)
    }
}

enum WaitEntry<V> {
    Waiting(WakerSet),
    Filled(V),
}

impl<V> WaitEntry<V> {
    fn remove_value(self) -> V {
        match self {
            Waiting(_) => unreachable!("remove_value cal only be called if entry exists"),
            Filled(v) => v,
        }
    }
}

pub struct Wait<'a, 'b, K, V, S, Q>
where
    K: Hash + Eq + Borrow<Q>,
    S: BuildHasher + Clone,
    Q: ?Sized + Hash + Eq,
{
    map: &'a DashMap<K, WaitEntry<V>, S>,
    key: &'b Q,
    idx: usize,
}

impl<'a, 'b, K, V, S, Q> Wait<'a, 'b, K, V, S, Q>
where
    K: Hash + Eq + Borrow<Q>,
    S: BuildHasher + Clone,
    Q: ?Sized + Hash + Eq,
{
    pub(crate) fn new(map: &'a DashMap<K, WaitEntry<V>, S>, key: &'b Q) -> Self {
        Wait {
            map,
            key,
            idx: std::usize::MAX,
        }
    }
}

impl<'a, 'b, K, V, S, Q> Future for Wait<'a, 'b, K, V, S, Q>
where
    K: Hash + Eq + Borrow<Q>,
    S: BuildHasher + Clone,
    Q: ?Sized + Hash + Eq,
{
    type Output = Option<V>;

    fn poll(mut self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.map.entry(self.key) {
            Entry::Occupied(mut entry) => match entry.get_mut() {
                Waiting(wakers) => {
                    wakers.replace(ctx.waker().clone(), &mut self.idx);
                    Poll::Pending
                }
                Filled(_) => {
                    self.idx = std::usize::MAX;
                    let wait_entry = entry.remove();
                    Poll::Ready(Some(wait_entry.remove_value()))
                }
            },
            Entry::Vacant(_) => Poll::Ready(None),
        }
    }
}

impl<'a, 'b, K, V, S, Q> Drop for Wait<'a, 'b, K, V, S, Q>
where
    K: Hash + Eq + Borrow<Q>,
    S: BuildHasher + Clone,
    Q: ?Sized + Hash + Eq,
{
    fn drop(&mut self) {
        if self.idx == std::usize::MAX {
            return;
        }
        if let Some(mut entry) = self.map.get_mut(self.key) {
            if let Waiting(wakers) = entry.value_mut() {
                wakers.remove(self.idx);
            }
        }
    }
}

pub struct WakerSet {
    wakers: SmallVec<[Option<Waker>; 1]>,
}

impl WakerSet {
    pub fn new() -> WakerSet {
        WakerSet {
            wakers: SmallVec::new(),
        }
    }

    pub fn replace(&mut self, waker: Waker, idx: &mut usize) {
        let len = self.wakers.len();
        if *idx >= len {
            debug_assert!(len != std::usize::MAX); // usize::MAX is used as a sentinel
            *idx = len;
            self.wakers.push(Some(waker));
        } else {
            self.wakers[*idx] = Some(waker);
        }
    }

    pub fn remove(&mut self, idx: usize) {
        self.wakers[idx] = None;
    }

    pub fn wake(self) {
        for waker in self.wakers {
            if let Some(waker) = waker {
                waker.wake()
            }
        }
    }
}
