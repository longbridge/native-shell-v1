//! Native child lists: what each native view holds, through flattened descendants.
//!
//! Only parents whose list may have changed are recomputed. Deleted children are
//! already gone (the renderer detaches a deleted view itself); the survivors keep
//! their relative order where they can: the longest run of survivors already in
//! the new order stays put and everything else is moved or inserted in front of
//! its successor, walking the new list from the end. Indices are those of the
//! renderer's list at the moment each op is applied.

use native_shell_core::protocol::{Mutation, ROOT_NODE};

use crate::{Batch, IdMap, Tree};

impl Tree {
    pub(crate) fn sync_native_children(&mut self, b: &mut Batch) {
        let mut dirty: Vec<(u32, u32)> = b
            .dirty_native
            .iter()
            .filter_map(|id| match *id {
                ROOT_NODE => Some((0, ROOT_NODE)),
                id => self
                    .nodes
                    .get(&id)
                    .filter(|n| n.native.is_some())
                    .map(|n| (n.depth, id)),
            })
            .collect();
        // Parents before children: a child list is only touched once its owner is attached.
        dirty.sort_unstable();
        for (_, parent) in dirty {
            let new = self.collect_native_children(parent);
            let old = self.native_children.remove(&parent).unwrap_or_default();
            let current: Vec<u32> = old
                .into_iter()
                .filter(|c| self.nodes.contains_key(c))
                .collect();
            diff_children(parent, current, &new, &mut b.structure);
            self.native_children.insert(parent, new);
        }
    }

    fn collect_native_children(&self, parent: u32) -> Vec<u32> {
        if parent == ROOT_NODE {
            return self.surfaces.clone();
        }
        let mut out = Vec::new();
        if let Some(n) = self.nodes.get(&parent) {
            self.collect_into(&n.children, &mut out);
        }
        out
    }

    fn collect_into(&self, children: &[u32], out: &mut Vec<u32>) {
        for c in children {
            let Some(n) = self.nodes.get(c) else { continue };
            if n.native.is_some() {
                out.push(*c);
            } else {
                self.collect_into(&n.children, out);
            }
        }
    }
}

/// Emits the ops that turn `current` (the renderer's list after deletes) into `new`.
pub(crate) fn diff_children(
    parent: u32,
    mut current: Vec<u32>,
    new: &[u32],
    out: &mut Vec<Mutation>,
) {
    if current == new {
        return;
    }
    if current.is_empty() {
        for (i, id) in new.iter().enumerate() {
            out.push(Mutation::Insert {
                node: *id,
                parent,
                index: i as u32,
            });
        }
        return;
    }
    let pos: IdMap<usize> = new.iter().enumerate().map(|(i, id)| (*id, i)).collect();
    // A survivor that is not in the new list would need reparenting, which the tree never does.
    let mut i = 0;
    while i < current.len() {
        if pos.contains_key(&current[i]) {
            i += 1;
        } else {
            debug_assert!(
                false,
                "native child {} left parent {parent} without being deleted",
                current[i]
            );
            out.push(Mutation::Remove {
                node: current[i],
                parent,
                index: i as u32,
            });
            current.remove(i);
        }
    }
    let seq: Vec<usize> = current.iter().map(|id| pos[id]).collect();
    if seq.windows(2).all(|w| w[0] < w[1]) {
        // Survivors already in order: only insertions.
        let mut j = 0;
        for (i, id) in new.iter().enumerate() {
            if j < current.len() && current[j] == *id {
                j += 1;
            } else {
                out.push(Mutation::Insert {
                    node: *id,
                    parent,
                    index: i as u32,
                });
            }
        }
        return;
    }
    let stable = longest_increasing(&seq);
    let stable: IdMap<()> = stable.into_iter().map(|k| (current[k], ())).collect();
    let present: IdMap<()> = current.iter().map(|id| (*id, ())).collect();
    let mut anchor = current.len();
    for id in new.iter().rev() {
        if !present.contains_key(id) {
            out.push(Mutation::Insert {
                node: *id,
                parent,
                index: anchor as u32,
            });
            current.insert(anchor, *id);
        } else if stable.contains_key(id) {
            anchor = current.iter().position(|c| c == id).unwrap_or(anchor);
        } else {
            let from = current.iter().position(|c| c == id).unwrap_or(0);
            let to = if from < anchor { anchor - 1 } else { anchor };
            if from != to {
                out.push(Mutation::Move {
                    node: *id,
                    parent,
                    from: from as u32,
                    to: to as u32,
                });
                let moved = current.remove(from);
                current.insert(to, moved);
            }
            anchor = to;
        }
    }
    debug_assert_eq!(current, new);
}

/// Indices into `seq` of one longest strictly increasing subsequence.
fn longest_increasing(seq: &[usize]) -> Vec<usize> {
    // tails[k]: index in seq of the smallest tail of an increasing run of length k + 1.
    let mut tails: Vec<usize> = Vec::new();
    let mut prev: Vec<Option<usize>> = vec![None; seq.len()];
    for (i, v) in seq.iter().enumerate() {
        let k = tails.partition_point(|&t| seq[t] < *v);
        if k > 0 {
            prev[i] = Some(tails[k - 1]);
        }
        if k == tails.len() {
            tails.push(i);
        } else {
            tails[k] = i;
        }
    }
    let mut out = Vec::with_capacity(tails.len());
    let mut at = tails.last().copied();
    while let Some(i) = at {
        out.push(i);
        at = prev[i];
    }
    out.reverse();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(mut list: Vec<u32>, ops: &[Mutation]) -> Vec<u32> {
        for op in ops {
            match op {
                Mutation::Insert { node, index, .. } => list.insert(*index as usize, *node),
                Mutation::Move { from, to, .. } => {
                    let n = list.remove(*from as usize);
                    list.insert(*to as usize, n);
                }
                Mutation::Remove { index, .. } => {
                    list.remove(*index as usize);
                }
                _ => unreachable!(),
            }
        }
        list
    }

    #[test]
    fn every_permutation_of_five_round_trips_with_few_moves() {
        let base = [1u32, 2, 3, 4, 5];
        let mut perm = base.to_vec();
        let mut count = 0;
        // Heap's algorithm over all 120 orders, each also with an inserted node.
        fn heap(k: usize, a: &mut Vec<u32>, f: &mut dyn FnMut(&[u32])) {
            if k == 1 {
                f(a);
                return;
            }
            heap(k - 1, a, f);
            for i in 0..k - 1 {
                if k.is_multiple_of(2) {
                    a.swap(i, k - 1);
                } else {
                    a.swap(0, k - 1);
                }
                heap(k - 1, a, f);
            }
        }
        heap(5, &mut perm, &mut |p| {
            count += 1;
            for with_new in [false, true] {
                let mut new = p.to_vec();
                if with_new {
                    new.insert(2, 9);
                }
                let mut ops = Vec::new();
                diff_children(0, base.to_vec(), &new, &mut ops);
                assert_eq!(apply(base.to_vec(), &ops), new, "{p:?}");
                let moves = ops
                    .iter()
                    .filter(|o| matches!(o, Mutation::Move { .. }))
                    .count();
                let seq: Vec<usize> = base
                    .iter()
                    .map(|b| p.iter().position(|x| x == b).unwrap())
                    .collect();
                assert!(moves <= 5 - longest_increasing(&seq).len(), "{p:?}");
            }
        });
        assert_eq!(count, 120);
    }

    #[test]
    fn moving_first_to_last_is_one_move() {
        let mut ops = Vec::new();
        diff_children(7, vec![1, 2, 3, 4], &[2, 3, 4, 1], &mut ops);
        assert_eq!(
            ops,
            vec![Mutation::Move {
                node: 1,
                parent: 7,
                from: 0,
                to: 3
            }]
        );
    }
}
