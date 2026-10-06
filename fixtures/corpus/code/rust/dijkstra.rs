use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// Weighted directed graph stored as an adjacency list.
pub struct Graph {
    adj: Vec<Vec<(usize, u64)>>,
}

impl Graph {
    pub fn new(n: usize) -> Self {
        Self { adj: vec![Vec::new(); n] }
    }

    pub fn add_edge(&mut self, from: usize, to: usize, weight: u64) {
        self.adj[from].push((to, weight));
    }

    /// Shortest distances from `source` to every node using a binary heap.
    /// Unreachable nodes get `None`.
    pub fn shortest_paths(&self, source: usize) -> Vec<Option<u64>> {
        let mut dist: Vec<Option<u64>> = vec![None; self.adj.len()];
        let mut heap = BinaryHeap::new();
        dist[source] = Some(0);
        heap.push(Reverse((0u64, source)));

        while let Some(Reverse((d, node))) = heap.pop() {
            if dist[node].is_some_and(|best| d > best) {
                continue; // stale entry
            }
            for &(next, w) in &self.adj[node] {
                let candidate = d + w;
                if dist[next].map_or(true, |cur| candidate < cur) {
                    dist[next] = Some(candidate);
                    heap.push(Reverse((candidate, next)));
                }
            }
        }
        dist
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_cheaper_indirect_route() {
        let mut g = Graph::new(4);
        g.add_edge(0, 1, 10);
        g.add_edge(0, 2, 3);
        g.add_edge(2, 1, 4);
        g.add_edge(1, 3, 2);
        let d = g.shortest_paths(0);
        assert_eq!(d, vec![Some(0), Some(7), Some(3), Some(9)]);
    }
}
