use std::collections::HashMap;

// ── BK-tree for O(n log n) pHash similarity search ───────────────────────
// A BK-tree organises hashes in a metric tree keyed by Hamming distance.
// Querying for "all hashes within distance T of X" visits only a fraction
// of nodes (O(log n + k)) instead of scanning everything (O(n)).
pub(super) struct BkNode {
    pub(super) ph_idx:   usize,                // index into ph_pairs
    pub(super) hash:     image_hasher::ImageHash,
    pub(super) children: HashMap<u32, usize>, // hamming distance → arena index
}

pub(super) struct BkTree {
    pub(super) nodes: Vec<BkNode>,
}

impl BkTree {
    pub(super) fn new(capacity: usize) -> Self {
        Self { nodes: Vec::with_capacity(capacity) }
    }

    pub(super) fn insert(&mut self, ph_idx: usize, hash: image_hasher::ImageHash) {
        if self.nodes.is_empty() {
            self.nodes.push(BkNode { ph_idx, hash, children: HashMap::new() });
            return;
        }
        let mut cur = 0;
        loop {
            let d = self.nodes[cur].hash.dist(&hash);
            // Separate immutable get from mutable insert to satisfy borrow checker.
            if let Some(&next) = self.nodes[cur].children.get(&d) {
                cur = next;
            } else {
                let new_idx = self.nodes.len();
                self.nodes[cur].children.insert(d, new_idx);
                self.nodes.push(BkNode { ph_idx, hash, children: HashMap::new() });
                return;
            }
        }
    }

    /// Returns ph_pairs indices for all entries with Hamming distance ≤ threshold.
    pub(super) fn query(&self, query: &image_hasher::ImageHash, threshold: u32) -> Vec<usize> {
        let mut results = Vec::new();
        if self.nodes.is_empty() { return results; }
        let mut stack = vec![0usize];
        while let Some(node_idx) = stack.pop() {
            let node = &self.nodes[node_idx];
            let d = node.hash.dist(query);
            if d <= threshold { results.push(node.ph_idx); }
            // Triangle inequality prunes subtrees that cannot contain matches.
            let lo = d.saturating_sub(threshold);
            let hi = d.saturating_add(threshold);
            for (&child_dist, &child_idx) in &node.children {
                if child_dist >= lo && child_dist <= hi {
                    stack.push(child_idx);
                }
            }
        }
        results
    }
}
