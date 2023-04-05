use std::{any::type_name, collections::HashMap, fmt::Debug};

/// HashMap wrapper with incremental ID (u64) assignment.
pub struct HashMapId<T> {
    id_seed: u64,
    store: HashMap<u64, T>,
}

impl<T> HashMapId<T>
where
    T: Send + Sync,
{
    pub fn new() -> Self {
        Self {
            id_seed: 0,
            store: HashMap::new(),
        }
    }

    pub fn add(&mut self, item: T) -> u64 {
        let id = self.id_seed;
        self.store.insert(id, item);
        self.id_seed += 1;
        id
    }

    pub fn remove(&mut self, id: u64) -> Option<T> {
        self.store.remove(&id)
    }

    pub fn get_mut(&mut self, id: u64) -> Option<&mut T> {
        self.store.get_mut(&id)
    }

    pub fn get(&self, id: u64) -> Option<&T> {
        self.store.get(&id)
    }

    pub fn get_last(&self) -> Option<&T> {
        self.store
            .iter()
            .max_by_key(|(k, _)| **k)
            .map(|(_, value)| value)
    }
}

impl<T> Default for HashMapId<T>
where
    T: Send + Sync,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Debug for HashMapId<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HashMapId")
            .field("id_seed", &self.id_seed)
            .field("type", &type_name::<T>())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // get

    #[test]
    fn get_returns_none_for_non_existent_item() {
        let hash: HashMapId<i32> = HashMapId::new();
        let item = hash.get(10);
        assert!(item.is_none());
    }

    #[test]
    fn get_returns_reference_to_item() {
        let mut hash: HashMapId<i32> = HashMapId::new();
        let value = 10;
        let id = hash.add(value);
        let item = hash.get(id);
        assert_eq!(item, Some(&value));
    }

    // get_mut

    #[test]
    fn get_mut_returns_mutable_reference() {
        let mut hash: HashMapId<i32> = HashMapId::new();
        let value = 10;
        let id = hash.add(value);
        let item = hash.get_mut(id);
        assert_eq!(item, Some(&mut value.clone()));
    }

    #[test]
    fn get_mut_returns_none_for_non_existent_item() {
        let mut hash: HashMapId<i32> = HashMapId::new();
        let item = hash.get_mut(0);
        assert!(item.is_none());
    }

    // add

    #[test]
    fn add_adds_item_to_store() {
        let mut hash: HashMapId<i32> = HashMapId::new();
        let item = 10;
        let id = hash.add(item);
        assert_eq!(hash.get(id), Some(&item));
    }

    #[test]
    fn add_can_add_multiple_items() {
        let mut hash: HashMapId<i32> = HashMapId::new();
        let item1 = 10;
        let item2 = 20;
        let id1 = hash.add(item1);
        let id2 = hash.add(item2);
        assert_ne!(id1, id2);
        assert_eq!(hash.get(id1), Some(&item1));
        assert_eq!(hash.get(id2), Some(&item2));
    }

    #[test]
    fn add_increments_id() {
        let mut hash: HashMapId<i32> = HashMapId::new();
        let id1 = hash.add(10);
        let id2 = hash.add(20);
        assert_eq!(id2, id1 + 1);
    }

    #[test]
    #[should_panic]
    fn add_panics_on_integer_overflow() {
        let mut hash: HashMapId<i32> = HashMapId::new();
        let item = 10;
        hash.id_seed = std::u64::MAX;
        hash.add(item);
    }

    // remove

    #[test]
    fn remove_handles_non_existent_id() {
        let mut hash: HashMapId<i32> = HashMapId::new();
        let result = hash.remove(1);
        assert!(result.is_none());
    }

    #[test]
    fn remove_returns_removed_value() {
        let mut hash: HashMapId<i32> = HashMapId::new();
        let value = 10;
        let id = hash.add(value);
        let removed_value = hash.remove(id);
        assert_eq!(removed_value, Some(value));
    }

    #[test]
    fn remove_removes_id() {
        let mut hash: HashMapId<i32> = HashMapId::new();
        let id = hash.add(10);
        hash.remove(id);
        assert_eq!(hash.get(id), None);
    }

    #[test]
    fn remove_does_not_affect_other_items() {
        let mut hash: HashMapId<i32> = HashMapId::new();

        let value1 = 10;
        let value2 = 20;
        let value3 = 30;

        let id1 = hash.add(value1);
        let id2 = hash.add(value2);
        let id3 = hash.add(value3);

        hash.remove(id2);

        assert_eq!(hash.get(id1), Some(&value1));
        assert!(hash.get(id2).is_none());
        assert_eq!(hash.get(id3), Some(&value3));
    }

    // impl

    #[test]
    fn default_creates_new_hashmapid() {
        let hash: HashMapId<i32> = HashMapId::default();
        assert_eq!(hash.id_seed, 0);
        assert!(hash.store.is_empty());
    }

    // fmt

    #[test]
    fn fmt_formats_the_hashmapid() {
        let mut hash: HashMapId<i32> = HashMapId::default();
        hash.add(10);

        let expected = format!("HashMapId {{ id_seed: {}, type: \"i32\" }}", hash.id_seed);
        let result = format!("{:?}", hash);

        assert_eq!(result, expected);
    }
}
