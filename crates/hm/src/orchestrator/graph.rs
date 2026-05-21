//! Chain-bounded DAG built from a wire-typed `Pipeline`.
//!
//! Each `Node` carries an owned wire `CommandStep`, so the scheduler
//! reads `runner`/`runner_args` directly at dispatch time — no
//! legacy schema round-trip.
//!
//! Two edge sets at the Node level:
//!
//! * `builds_in` — at most one parent per step. Lineage edge: child runs
//!   inside the parent's container (locally) or boots from the parent's
//!   snapshot (remotely). Implies a sync edge.
//!
//! * `depends_on` — synchronisation edges (no state inheritance).
//!   Computed from the `builds_in` parent (always added) plus any
//!   implicit barriers introduced by `wait` steps in the wire format.
//!   v0 has no schema-level `depends_on` — chain DSL is the only
//!   topology mechanism.
//!
//! After flattening wait barriers, the graph is a DAG over command
//! steps. A "lineage chain" is a maximal path of `builds_in` edges
//! with single-child branching at every internal node — these are
//! schedulable as a single long-lived container.

use std::collections::BTreeMap;

use anyhow::Result;
use hm_plugin_protocol::{CommandStep, Pipeline, Step};

#[derive(Debug, Clone)]
pub struct Node {
    pub step: CommandStep,
    /// Resolved final environment (pipeline.env merged with step.env).
    pub env: BTreeMap<String, String>,
    /// `builds_in` parent's index, if any.
    pub builds_in: Option<usize>,
    /// Synchronisation edges (computed from `builds_in` and wait barriers).
    pub depends_on: Vec<usize>,
    /// `builds_in` children's indices.
    pub builds_in_children: Vec<usize>,
}

#[derive(Debug, Clone)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub default_image: Option<String>,
}

/// A flattened command step plus the keys of any prior steps a `wait`
/// barrier inserted between them and us. The `extra_deps` are step
/// keys, not graph indices — they're resolved during graph build.
struct FlatStep {
    step: CommandStep,
    extra_deps: Vec<String>,
}

impl Graph {
    /// Build a synchronisation graph from a planner-emitted `Pipeline`.
    ///
    /// # Errors
    ///
    /// Returns an error if a step's `builds_in` or `depends_on`
    /// references an unknown step key, or if the resulting graph is
    /// cyclic. Each cycle error names both ends of the back edge so the
    /// user can grep their pipeline for the offending pair.
    pub fn build(pipeline: &Pipeline) -> Result<Self> {
        let flat = flatten_steps(&pipeline.steps);
        let key_to_idx: BTreeMap<String, usize> = flat
            .iter()
            .enumerate()
            .map(|(i, f)| (f.step.key.clone(), i))
            .collect();
        let pipeline_env = pipeline.env.clone().unwrap_or_default();

        let mut nodes: Vec<Node> = flat
            .iter()
            .map(|f| {
                let mut env = pipeline_env.clone();
                if let Some(e) = &f.step.env {
                    env.extend(e.clone());
                }
                Node {
                    step: f.step.clone(),
                    env,
                    builds_in: None,
                    depends_on: vec![],
                    builds_in_children: vec![],
                }
            })
            .collect();

        for (i, f) in flat.iter().enumerate() {
            if let Some(parent_key) = &f.step.builds_in {
                let p = *key_to_idx.get(parent_key).ok_or_else(|| {
                    anyhow::anyhow!(
                        "step '{}' builds_in references unknown step '{}'",
                        f.step.key,
                        parent_key
                    )
                })?;
                nodes[i].builds_in = Some(p);
                nodes[p].builds_in_children.push(i);
                if !nodes[i].depends_on.contains(&p) {
                    nodes[i].depends_on.push(p);
                }
            }
            for dep_key in &f.extra_deps {
                let p = *key_to_idx.get(dep_key).ok_or_else(|| {
                    anyhow::anyhow!(
                        "step '{}' has wait-barrier dep on unknown step '{}'",
                        f.step.key,
                        dep_key
                    )
                })?;
                if !nodes[i].depends_on.contains(&p) {
                    nodes[i].depends_on.push(p);
                }
            }
        }

        // Root steps (no `builds_in`) with no explicit `image` inherit
        // the pipeline's `default_image`. Without this the docker
        // plugin's `image_name::resolve_image` falls back to
        // `alpine:latest`, which breaks every apt-based example. Patch
        // at the host so the plugin stays pipeline-agnostic.
        if let Some(default_img) = pipeline.default_image.as_deref() {
            for node in &mut nodes {
                if node.builds_in.is_none() && node.step.image.is_none() {
                    node.step.image = Some(default_img.to_string());
                }
            }
        }

        let g = Self {
            nodes,
            default_image: pipeline.default_image.clone(),
        };
        g.assert_acyclic()?;
        Ok(g)
    }

    fn assert_acyclic(&self) -> Result<()> {
        // Iterative DFS with grey/black colouring. When we enter a grey
        // node, the parent we came from is the back-edge source; the
        // grey node itself is the back-edge target.
        let mut color = vec![0u8; self.nodes.len()]; // 0 white, 1 grey, 2 black
        for start in 0..self.nodes.len() {
            if color[start] == 0 {
                let mut stack: Vec<(usize, Option<usize>, bool)> = vec![(start, None, false)];
                while let Some((n, parent, exiting)) = stack.pop() {
                    if exiting {
                        color[n] = 2;
                        continue;
                    }
                    if color[n] == 1 {
                        let target = &self.nodes[n].step.key;
                        match parent {
                            Some(p) => anyhow::bail!(
                                "cycle: '{}' is reachable from itself via '{}'",
                                target,
                                self.nodes[p].step.key
                            ),
                            None => anyhow::bail!("cycle through step '{target}'"),
                        }
                    }
                    color[n] = 1;
                    stack.push((n, parent, true));
                    for &c in &self.nodes[n].depends_on {
                        if color[c] != 2 {
                            stack.push((c, Some(n), false));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// True iff `i` is the unique `builds_in` child of its parent and has
    /// no other `depends_on` edges. A chain step can run via `docker exec`
    /// in the parent's running container.
    #[must_use]
    pub fn is_chain_step(&self, i: usize) -> bool {
        // depends_on length == 1 because the only edge is the implied
        // builds_in dep.
        self.nodes[i].builds_in.is_some_and(|p| {
            self.nodes[p].builds_in_children.len() == 1 && self.nodes[i].depends_on.len() == 1
        })
    }

    /// For each chain in `chains`, return the set of OTHER chain indices
    /// it depends on. A chain depends on another chain iff any node in
    /// it has a `depends_on` edge into a node belonging to that other
    /// chain. The result is deterministic (ascending chain index).
    ///
    /// Caller must pass the same partition `chains()` returned —
    /// `chain_index[node]` is derived from `chains`.
    #[must_use]
    pub fn chain_deps(&self, chains: &[Vec<usize>]) -> Vec<Vec<usize>> {
        let mut chain_index = vec![usize::MAX; self.nodes.len()];
        for (ci, ch) in chains.iter().enumerate() {
            for &n in ch {
                chain_index[n] = ci;
            }
        }
        let mut out: Vec<Vec<usize>> = vec![Vec::new(); chains.len()];
        for (ci, ch) in chains.iter().enumerate() {
            let mut seen: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();
            for &n in ch {
                for &dep in &self.nodes[n].depends_on {
                    let dep_ci = chain_index[dep];
                    if dep_ci != ci {
                        seen.insert(dep_ci);
                    }
                }
            }
            out[ci] = seen.into_iter().collect();
        }
        out
    }

    /// Partition `nodes` into chain units. Each returned vec is a chain
    /// `[root, step_1, step_2, …]` where `root` is **not** a chain step
    /// and every subsequent index is a chain step descending from the
    /// previous one via `builds_in`. Every node appears in exactly one
    /// chain. The order of chains is deterministic (root index, ascending).
    #[must_use]
    pub fn chains(&self) -> Vec<Vec<usize>> {
        let n = self.nodes.len();
        let mut placed = vec![false; n];
        let mut out: Vec<Vec<usize>> = Vec::new();
        for root in 0..n {
            if placed[root] || self.is_chain_step(root) {
                continue;
            }
            let mut chain = vec![root];
            placed[root] = true;
            // Follow the unique chain-step child, if any.
            let mut cur = root;
            while let Some(&next) = self.nodes[cur]
                .builds_in_children
                .iter()
                .find(|&&c| self.is_chain_step(c))
            {
                chain.push(next);
                placed[next] = true;
                cur = next;
            }
            out.push(chain);
        }
        out
    }
}

/// Flatten v0 steps. Drops Wait nodes; for each command step that
/// follows a wait, records the keys of the steps that completed
/// before that wait as `extra_deps`.
fn flatten_steps(steps: &[Step]) -> Vec<FlatStep> {
    let mut out: Vec<FlatStep> = Vec::new();
    let mut implicit_wait_targets: Vec<String> = Vec::new();
    for s in steps {
        match s {
            Step::Command(c) => {
                out.push(FlatStep {
                    step: (**c).clone(),
                    extra_deps: implicit_wait_targets.clone(),
                });
            }
            Step::Wait(_) => {
                implicit_wait_targets = out.iter().map(|f| f.step.key.clone()).collect();
            }
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// Decode a wire `Pipeline` directly from JSON bytes. Mirrors the
    /// legacy `local::schema::decode_plan` helper but uses the wire
    /// types so the new module has no dependency on the legacy schema.
    fn decode_wire(bytes: &[u8]) -> Pipeline {
        serde_json::from_slice::<Pipeline>(bytes).unwrap()
    }

    #[test]
    fn chain_detection() {
        let json = br#"{
          "version":"0",
          "default_image":"ubuntu:24.04",
          "steps":[
            {"type":"command","key":"a","cmd":"echo a"},
            {"type":"command","key":"b","cmd":"echo b","builds_in":"a"},
            {"type":"command","key":"c","cmd":"echo c","builds_in":"b"}
          ]
        }"#;
        let p = decode_wire(json);
        let g = Graph::build(&p).unwrap();
        assert!(!g.is_chain_step(0)); // root, no parent
        assert!(g.is_chain_step(1));
        assert!(g.is_chain_step(2));
    }

    #[test]
    fn fork_breaks_chain() {
        let json = br#"{
          "version":"0",
          "default_image":"ubuntu:24.04",
          "steps":[
            {"type":"command","key":"a","cmd":"echo a"},
            {"type":"command","key":"b","cmd":"echo b","builds_in":"a"},
            {"type":"command","key":"c","cmd":"echo c","builds_in":"a"}
          ]
        }"#;
        let p = decode_wire(json);
        let g = Graph::build(&p).unwrap();
        assert!(!g.is_chain_step(1)); // sibling exists, must commit a then run separately
        assert!(!g.is_chain_step(2));
    }

    #[test]
    fn wait_inserts_implicit_deps() {
        let json = br#"{
          "version":"0",
          "steps":[
            {"type":"command","key":"a","cmd":"echo a"},
            {"type":"command","key":"b","cmd":"echo b"},
            {"type":"wait"},
            {"type":"command","key":"c","cmd":"echo c"}
          ]
        }"#;
        let p = decode_wire(json);
        let g = Graph::build(&p).unwrap();
        // c (index 2 since the wait is dropped) should depend on a (0) and b (1).
        let c_idx = g.nodes.iter().position(|n| n.step.key == "c").unwrap();
        let a_idx = g.nodes.iter().position(|n| n.step.key == "a").unwrap();
        let b_idx = g.nodes.iter().position(|n| n.step.key == "b").unwrap();
        assert!(g.nodes[c_idx].depends_on.contains(&a_idx));
        assert!(g.nodes[c_idx].depends_on.contains(&b_idx));
    }

    #[test]
    fn rejects_unknown_builds_in() {
        let json = br#"{
          "version":"0",
          "steps":[
            {"type":"command","key":"b","cmd":"echo b","builds_in":"missing"}
          ]
        }"#;
        let p = decode_wire(json);
        let err = Graph::build(&p).unwrap_err();
        assert!(err.to_string().contains("missing") || err.to_string().contains("unknown"));
    }

    #[test]
    fn chains_partition_includes_every_node_once() {
        // Pipeline:
        //   a (root) -> b (chain) -> c (chain)
        //            -> d (fork)
        //   e (root, independent)
        let json = br#"{
          "version":"0",
          "default_image":"ubuntu:24.04",
          "steps":[
            {"type":"command","key":"a","cmd":"echo a"},
            {"type":"command","key":"b","cmd":"echo b","builds_in":"a"},
            {"type":"command","key":"c","cmd":"echo c","builds_in":"b"},
            {"type":"command","key":"d","cmd":"echo d","builds_in":"a"},
            {"type":"command","key":"e","cmd":"echo e"}
          ]
        }"#;
        let p = decode_wire(json);
        let g = Graph::build(&p).unwrap();
        let idx = |k: &str| g.nodes.iter().position(|n| n.step.key == k).unwrap();

        // 'a' has two builds_in children, so neither b nor d is a chain
        // step relative to a. But b -> c is a single-child chain.
        let chains = g.chains();
        let mut all_nodes: Vec<usize> = chains.iter().flatten().copied().collect();
        all_nodes.sort_unstable();
        assert_eq!(
            all_nodes,
            vec![idx("a"), idx("b"), idx("c"), idx("d"), idx("e")]
        );

        // The chain containing 'b' must also contain 'c' in that order.
        let bc_chain = chains
            .iter()
            .find(|ch| ch.contains(&idx("b")))
            .expect("b must be in some chain");
        assert_eq!(bc_chain, &vec![idx("b"), idx("c")]);

        // Every other chain is length-1.
        for ch in &chains {
            if ch != bc_chain {
                assert_eq!(ch.len(), 1, "non-bc chain not singleton: {ch:?}");
            }
        }
    }

    #[test]
    fn chain_deps_aggregates_cross_chain_edges() {
        // Pipeline:
        //   a -> b (chain) -> c (chain)
        //   a -> d (fork)
        //   e (independent)
        // Chains: [a], [b,c], [d], [e]
        // chain_deps: [a] none, [b,c] {[a]}, [d] {[a]}, [e] none
        let json = br#"{
          "version":"0",
          "steps":[
            {"type":"command","key":"a","cmd":"echo a"},
            {"type":"command","key":"b","cmd":"echo b","builds_in":"a"},
            {"type":"command","key":"c","cmd":"echo c","builds_in":"b"},
            {"type":"command","key":"d","cmd":"echo d","builds_in":"a"},
            {"type":"command","key":"e","cmd":"echo e"}
          ]
        }"#;
        let p = decode_wire(json);
        let g = Graph::build(&p).unwrap();
        let chains = g.chains();
        let deps = g.chain_deps(&chains);

        let find_chain = |key: &str| -> usize {
            let idx = g.nodes.iter().position(|n| n.step.key == key).unwrap();
            chains.iter().position(|ch| ch.contains(&idx)).unwrap()
        };
        let a_ci = find_chain("a");
        let bc_ci = find_chain("b");
        let d_ci = find_chain("d");
        let e_ci = find_chain("e");

        assert!(
            deps[a_ci].is_empty(),
            "chain a has no deps: {:?}",
            deps[a_ci]
        );
        assert_eq!(deps[bc_ci], vec![a_ci]);
        assert_eq!(deps[d_ci], vec![a_ci]);
        assert!(deps[e_ci].is_empty());
    }

    #[test]
    fn chain_deps_subsumes_wait_barriers() {
        let json = br#"{
          "version":"0",
          "steps":[
            {"type":"command","key":"a","cmd":"echo a"},
            {"type":"command","key":"b","cmd":"echo b"},
            {"type":"wait"},
            {"type":"command","key":"c","cmd":"echo c"}
          ]
        }"#;
        let p = decode_wire(json);
        let g = Graph::build(&p).unwrap();
        let chains = g.chains();
        let deps = g.chain_deps(&chains);
        let find_chain = |key: &str| -> usize {
            let idx = g.nodes.iter().position(|n| n.step.key == key).unwrap();
            chains.iter().position(|ch| ch.contains(&idx)).unwrap()
        };
        let a_ci = find_chain("a");
        let b_ci = find_chain("b");
        let c_ci = find_chain("c");
        let mut c_deps = deps[c_ci].clone();
        c_deps.sort_unstable();
        let mut want = vec![a_ci, b_ci];
        want.sort_unstable();
        assert_eq!(c_deps, want);
    }

    #[test]
    fn chains_root_is_never_a_chain_step() {
        let json = br#"{
          "version":"0",
          "steps":[
            {"type":"command","key":"a","cmd":"echo a"},
            {"type":"command","key":"b","cmd":"echo b","builds_in":"a"}
          ]
        }"#;
        let p = decode_wire(json);
        let g = Graph::build(&p).unwrap();
        for chain in g.chains() {
            let root = chain[0];
            assert!(!g.is_chain_step(root), "chain root {root} is a chain step");
            for &step in &chain[1..] {
                assert!(g.is_chain_step(step), "non-root {step} is not a chain step");
            }
        }
    }
}
