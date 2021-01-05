use std::collections::HashMap;

pub struct HashMapStore<T> {
    id_seed: u32,
    store: HashMap<u32, T>,
}

impl<T> HashMapStore<T> {
    pub fn new() -> Self {
        Self {
            id_seed: 0,
            store: HashMap::new(),
        }
    }

    pub fn add(&mut self, item: T) -> u32 {
        let id = self.id_seed;
        self.store.insert(id, item);
        self.id_seed += 1;
        id
    }

    pub fn remove(&mut self, id: u32) -> Option<T> {
        self.store.remove(&id)
    }

    pub fn get_mut(&mut self, id: u32) -> Option<&mut T> {
        self.store.get_mut(&id)
    }

    pub fn get(&self, id: u32) -> Option<&T> {
        self.store.get(&id)
    }
}
