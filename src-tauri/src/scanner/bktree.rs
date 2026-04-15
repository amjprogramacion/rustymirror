use std::collections::HashMap;

// ── BK-tree for O(n log n) pHash similarity search ───────────────────────
// Binary serialisation layout (little-endian):
//   [u32] node_count
//   per node:
//     [u32] ph_idx
//     [u8]  hash_len
//     [u8 × hash_len] hash bytes
//     [u32] children_count
//     per child: [u32] dist  [u32] child_idx
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

    /// Serialise the tree to a compact binary blob for SQLite storage.
    pub(super) fn serialize(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&(self.nodes.len() as u32).to_le_bytes());
        for node in &self.nodes {
            out.extend_from_slice(&(node.ph_idx as u32).to_le_bytes());
            let hb = node.hash.as_bytes();
            out.push(hb.len() as u8);
            out.extend_from_slice(hb);
            out.extend_from_slice(&(node.children.len() as u32).to_le_bytes());
            // Sort children for determinism across runs.
            let mut children: Vec<(u32, usize)> = node.children.iter().map(|(&d, &i)| (d, i)).collect();
            children.sort_unstable();
            for (dist, idx) in children {
                out.extend_from_slice(&dist.to_le_bytes());
                out.extend_from_slice(&(idx as u32).to_le_bytes());
            }
        }
        out
    }

    /// Deserialise a blob produced by `serialize`.  Returns `None` on any
    /// truncation or format mismatch so the caller can fall back to rebuild.
    pub(super) fn deserialize(data: &[u8]) -> Option<Self> {
        let mut p = 0usize;

        macro_rules! read_u32 {
            () => {{
                if p + 4 > data.len() { return None; }
                let v = u32::from_le_bytes([data[p], data[p+1], data[p+2], data[p+3]]);
                p += 4;
                v
            }};
        }

        let node_count = read_u32!() as usize;
        let mut nodes = Vec::with_capacity(node_count);

        for _ in 0..node_count {
            let ph_idx = read_u32!() as usize;

            if p >= data.len() { return None; }
            let hash_len = data[p] as usize;
            p += 1;

            if p + hash_len > data.len() { return None; }
            let hash = image_hasher::ImageHash::from_bytes(&data[p..p + hash_len]).ok()?;
            p += hash_len;

            let children_count = read_u32!() as usize;
            let mut children = HashMap::with_capacity(children_count);
            for _ in 0..children_count {
                let dist     = read_u32!();
                let child_idx = read_u32!() as usize;
                children.insert(dist, child_idx);
            }

            nodes.push(BkNode { ph_idx, hash, children });
        }

        // Any leftover bytes mean the blob is corrupt.
        if p != data.len() { return None; }
        Some(Self { nodes })
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
