use std::collections::HashMap;
use petgraph::graph::{DiGraph, NodeIndex};
use crate::manifest::ModManifest;

/// Errors that can occur during mod dependency resolution.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ResolverError {
    /// A required dependency was not found or failed the SemVer version requirement.
    #[error("Mod '{dependent}' requires '{required}' version '{requirement}', but it was not found or version does not match")]
    MissingDependency {
        dependent: String,
        required: String,
        requirement: String,
    },

    /// An incompatible mod was found with a conflicting version.
    #[error("Mod '{mod_a}' is incompatible with mod '{mod_b}' (found version {version_b})")]
    IncompatibleMod {
        mod_a: String,
        mod_b: String,
        version_b: String,
    },

    /// A circular dependency was detected.
    #[error("Circular dependency detected involving mods: {cycle:?}")]
    CircularDependency {
        cycle: Vec<String>,
    },

    /// Multiple manifests were supplied with the same mod ID.
    #[error("Duplicate mod ID found: '{id}'")]
    DuplicateModId {
        id: String,
    },
}

/// SemVer Directed Acyclic Graph (DAG) dependency resolver for mod manifests.
pub struct DependencyResolver;

impl DependencyResolver {
    /// Resolves and sorts mod manifests in topological order (dependencies first).
    ///
    /// Validates:
    /// 1. No duplicate mod IDs.
    /// 2. Every dependency is present and satisfies the declared `semver::VersionReq`.
    /// 3. No declared incompatibilities are violated.
    /// 4. No circular dependencies exist.
    pub fn resolve(manifests: Vec<ModManifest>) -> Result<Vec<ModManifest>, ResolverError> {
        // 1. Build lookup map and check for duplicate mod IDs.
        let mut manifest_map: HashMap<&str, usize> = HashMap::with_capacity(manifests.len());
        for (idx, manifest) in manifests.iter().enumerate() {
            if manifest_map.insert(manifest.id.as_str(), idx).is_some() {
                return Err(ResolverError::DuplicateModId {
                    id: manifest.id.clone(),
                });
            }
        }

        // 2. Validate all dependencies and version requirements.
        for manifest in &manifests {
            for (dep_id, req) in &manifest.dependencies {
                match manifest_map.get(dep_id.as_str()) {
                    None => {
                        return Err(ResolverError::MissingDependency {
                            dependent: manifest.id.clone(),
                            required: dep_id.clone(),
                            requirement: req.to_string(),
                        });
                    }
                    Some(&target_idx) => {
                        let target_manifest = &manifests[target_idx];
                        if !req.matches(&target_manifest.version) {
                            return Err(ResolverError::MissingDependency {
                                dependent: manifest.id.clone(),
                                required: dep_id.clone(),
                                requirement: req.to_string(),
                            });
                        }
                    }
                }
            }
        }

        // 3. Validate incompatibilities.
        for manifest in &manifests {
            for (incompat_id, req) in &manifest.incompatibilities {
                if let Some(&other_idx) = manifest_map.get(incompat_id.as_str()) {
                    let other_manifest = &manifests[other_idx];
                    if req.matches(&other_manifest.version) {
                        return Err(ResolverError::IncompatibleMod {
                            mod_a: manifest.id.clone(),
                            mod_b: incompat_id.clone(),
                            version_b: other_manifest.version.to_string(),
                        });
                    }
                }
            }
        }

        // 4. Build DAG where edge is dependency -> dependent (so dependency is sorted first).
        let mut graph: DiGraph<usize, ()> = DiGraph::with_capacity(manifests.len(), manifests.len() * 2);
        let node_indices: Vec<NodeIndex> = (0..manifests.len())
            .map(|idx| graph.add_node(idx))
            .collect();

        for (idx, manifest) in manifests.iter().enumerate() {
            let dependent_node = node_indices[idx];
            for dep_id in manifest.dependencies.keys() {
                let dep_idx = manifest_map[dep_id.as_str()];
                let dependency_node = node_indices[dep_idx];
                // dependency must come before dependent
                graph.add_edge(dependency_node, dependent_node, ());
            }
        }

        // 5. Detect cycles and compute topological ordering.
        match petgraph::algo::toposort(&graph, None) {
            Ok(sorted_nodes) => {
                let mut manifest_options: Vec<Option<ModManifest>> = manifests.into_iter().map(Some).collect();
                let mut result = Vec::with_capacity(manifest_options.len());
                for node in sorted_nodes {
                    let manifest_idx = graph[node];
                    result.push(manifest_options[manifest_idx].take().expect("Valid node index"));
                }
                Ok(result)
            }
            Err(_) => {
                // Find cycle nodes using DFS traversal
                let mut visited = vec![0u8; graph.node_count()]; // 0: unvisited, 1: visiting, 2: visited
                let mut stack = Vec::new();

                for node in graph.node_indices() {
                    if visited[node.index()] == 0
                        && let Some(cycle_indices) = Self::find_cycle_dfs(node, &graph, &mut visited, &mut stack) {
                            let mut cycle: Vec<String> = cycle_indices
                                .into_iter()
                                .map(|idx| manifests[idx].id.clone())
                                .collect();
                            cycle.sort();
                            return Err(ResolverError::CircularDependency { cycle });
                        }
                }

                Err(ResolverError::CircularDependency { cycle: Vec::new() })
            }
        }
    }

    fn find_cycle_dfs(
        node: NodeIndex,
        graph: &DiGraph<usize, ()>,
        visited: &mut [u8],
        stack: &mut Vec<NodeIndex>,
    ) -> Option<Vec<usize>> {
        visited[node.index()] = 1;
        stack.push(node);

        for neighbor in graph.neighbors(node) {
            if visited[neighbor.index()] == 1 {
                // Cycle found: slice stack from neighbor to current node
                if let Some(pos) = stack.iter().position(|&n| n == neighbor) {
                    let cycle = stack[pos..].iter().map(|n| graph[*n]).collect();
                    return Some(cycle);
                }
            } else if visited[neighbor.index()] == 0
                && let Some(cycle) = Self::find_cycle_dfs(neighbor, graph, visited, stack) {
                    return Some(cycle);
                }
        }

        stack.pop();
        visited[node.index()] = 2;
        None
    }
}
