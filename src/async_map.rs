use std::future::Future;
use std::hash::Hash;
use std::pin::Pin;
use std::task::Waker;
use std::task::{Context, Poll};

use dashmap::mapref::entry::Entry;
use dashmap::DashMap;

/// Async concurrent hashmap built on top of [dashmap](https://docs.rs/dashmap/).
/// It allows you to wait on a key until someone puts a value under this key into the map.
pub struct AsyncMap<K, V> {
    map: DashMap<K, WaitEntry<V>>,
}

impl<K: Hash + Eq + Copy, V> Default for AsyncMap<K, V> {
    fn default() -> Self {
        Self {
            map: DashMap::new(),
        }
    }
}

impl<K: Hash + Eq + Copy, V> AsyncMap<K, V> {
    /// Inserts a key-value pair into the map.
    ///
    /// If the map did not have this key present, `None` is returned.
    ///
    /// If there are any pending `wait_remove` calls for this key, they are woken up.
    ///
    /// If the map did have this key present, the value is updated and the old value is returned.
    pub fn insert(&self, key: K, value: V) -> Option<V> {
        match self.map.entry(key) {
            Entry::Occupied(mut entry) => {
                match std::mem::replace(entry.get_mut(), WaitEntry::Filled(value)) {
                    WaitEntry::Waiting(waker) => {
                        drop(entry); // drop early to release lock before waking other tasks
                        waker.wake();
                        None
                    }
                    WaitEntry::Filled(value) => Some(value),
                }
            }
            Entry::Vacant(slot) => {
                slot.insert(WaitEntry::Filled(value));
                None
            }
        }
    }

    /// Blocks until a value under `key` is present in the map.
    ///
    /// Panics if it's called twice on the same key.
    pub async fn wait_remove(&self, key: K) -> V {
        match self.map.entry(key) {
            Entry::Occupied(o_slot) => match o_slot.remove() {
                WaitEntry::Waiting(_) => panic!("already waiting on the key"),
                WaitEntry::Filled(v) => v,
            },
            Entry::Vacant(v_slot) => {
                // drop early to release lock before usint the entry API again in `Wait::poll`.
                drop(v_slot);
                Wait {
                    map: &self.map,
                    key,
                }
                .await
            }
        }
    }
}

enum WaitEntry<V> {
    Waiting(Waker),
    Filled(V),
}

impl<V> WaitEntry<V> {
    // Returns the value if it's filled, panics otherwise.
    fn value(self) -> V {
        match self {
            WaitEntry::Waiting(_) => panic!("remove called on vacant value"),
            WaitEntry::Filled(v) => v,
        }
    }
}

pub struct Wait<'a, K, V> {
    map: &'a DashMap<K, WaitEntry<V>>,
    key: K,
}

impl<'a, K: Hash + Eq + Copy, V> Future for Wait<'a, K, V> {
    type Output = V;

    fn poll(self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.map.entry(self.key) {
            Entry::Occupied(mut entry) => match entry.get_mut() {
                WaitEntry::Waiting(waker) => {
                    let _ = std::mem::replace(waker, ctx.waker().clone());
                    Poll::Pending
                }
                WaitEntry::Filled(_) => {
                    let wait_entry = entry.remove();
                    Poll::Ready(wait_entry.value())
                }
            },
            Entry::Vacant(slot) => {
                slot.insert(WaitEntry::Waiting(ctx.waker().clone()));
                Poll::Pending
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        future::Future,
        sync::{Arc, Mutex},
        task::{Context, Poll, Wake},
    };

    use super::AsyncMap;

    #[derive(Clone)]
    struct FlagWaker(Arc<Mutex<bool>>);
    impl Wake for FlagWaker {
        fn wake(self: Arc<Self>) {
            let mut called = self.0.lock().unwrap();
            *called = true;
        }
    }

    #[async_std::test]
    async fn simple_insert_test() {
        let async_map = AsyncMap::default();
        let value = async_map.wait_remove(0);
        let mut fut = Box::pin(value);
        // Manually poll future before insert
        let waker = FlagWaker(Arc::new(Mutex::new(false)));
        let waker_ref = waker.clone();
        let waker = &Arc::new(waker).into();
        let mut context = Context::from_waker(waker);
        let result = fut.as_mut().poll(&mut context);
        assert_eq!(result, Poll::Pending);
        // Insert
        async_map.insert(0, 1337);
        assert_eq!(*waker_ref.0.lock().unwrap(), true);
        // Manually poll future after insert
        let result = fut.as_mut().poll(&mut context);
        assert_eq!(result, Poll::Ready(1337));
    }
}
