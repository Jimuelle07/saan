use std::collections::{HashMap, VecDeque};
use std::hash::Hash;

/// A simple least-recently-used cache with a fixed capacity.
/// Recency is tracked with a deque; good enough for small capacities.
pub struct LruCache<K, V> {
    capacity: usize,
    map: HashMap<K, V>,
    order: VecDeque<K>,
}

impl<K: Eq + Hash + Clone, V> LruCache<K, V> {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "capacity must be positive");
        Self { capacity, map: HashMap::new(), order: VecDeque::new() }
    }

    fn touch(&mut self, key: &K) {
        if let Some(pos) = self.order.iter().position(|k| k == key) {
            let k = self.order.remove(pos).unwrap();
            self.order.push_back(k);
        }
    }

    pub fn get(&mut self, key: &K) -> Option<&V> {
        if self.map.contains_key(key) {
            self.touch(key);
        }
        self.map.get(key)
    }

    pub fn put(&mut self, key: K, value: V) {
        if self.map.contains_key(&key) {
            self.touch(&key);
        } else {
            if self.map.len() == self.capacity {
                if let Some(oldest) = self.order.pop_front() {
                    self.map.remove(&oldest);
                }
            }
            self.order.push_back(key.clone());
        }
        self.map.insert(key, value);
    }
}

#[test]
fn evicts_least_recently_used() {
    let mut c = LruCache::new(2);
    c.put("a", 1);
    c.put("b", 2);
    c.get(&"a");
    c.put("c", 3);
    assert!(c.get(&"b").is_none());
    assert_eq!(c.get(&"a"), Some(&1));
}
