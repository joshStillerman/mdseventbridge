use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct SeenCache {
    ttl: Duration,
    max_entries: usize,
    order: VecDeque<(Instant, String)>,
    entries: HashMap<String, Instant>,
}

impl SeenCache {
    #[must_use]
    pub fn new(ttl: Duration, max_entries: usize) -> Self {
        Self {
            ttl,
            max_entries,
            order: VecDeque::new(),
            entries: HashMap::new(),
        }
    }

    pub fn add_if_new(&mut self, id: &str, now: Instant) -> bool {
        self.prune(now);
        if self.entries.contains_key(id) {
            return false;
        }
        let id_owned = id.to_string();
        self.entries.insert(id_owned.clone(), now);
        self.order.push_back((now, id_owned));
        self.enforce_capacity();
        true
    }

    fn prune(&mut self, now: Instant) {
        let expire_before = now.checked_sub(self.ttl).unwrap_or(now);
        while let Some((ts, id)) = self.order.front().cloned() {
            if ts >= expire_before {
                break;
            }
            self.order.pop_front();
            let remove = self
                .entries
                .get(&id)
                .is_some_and(|stored| *stored <= expire_before);
            if remove {
                self.entries.remove(&id);
            }
        }
    }

    fn enforce_capacity(&mut self) {
        while self.entries.len() > self.max_entries {
            if let Some((_ts, id)) = self.order.pop_front() {
                self.entries.remove(&id);
            } else {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::SeenCache;

    #[test]
    fn dedupe_basics() {
        let mut cache = SeenCache::new(Duration::from_secs(1), 3);
        let now = Instant::now();
        assert!(cache.add_if_new("a", now));
        assert!(!cache.add_if_new("a", now));
        assert!(cache.add_if_new("b", now));
        assert!(cache.add_if_new("c", now));
        assert!(cache.add_if_new("d", now));
        assert!(cache.add_if_new("a", now + Duration::from_secs(2)));
    }
}
