//! 4-Color Spatial Domain Decomposition for lock-free parallel chunk simulation.

/// Computes the 4-color domain index (0, 1, 2, or 3) for chunk coordinate (x, z).
///
/// Uses Euclidean modulo to ensure consistent tiling across negative coordinates:
/// `((x.rem_euclid(2)) + 2 * (z.rem_euclid(2))) as usize`
#[inline]
pub fn chunk_domain_color(x: i32, z: i32) -> usize {
    let mod_x = x.rem_euclid(2) as usize;
    let mod_z = z.rem_euclid(2) as usize;
    mod_x + 2 * mod_z
}

/// Holds sets of chunk coordinates partitioned by domain color (0..4).
#[derive(Debug, Clone, Default)]
pub struct DomainPartition {
    domains: [Vec<(i32, i32)>; 4],
}

impl DomainPartition {
    pub fn new() -> Self {
        Self {
            domains: [
                Vec::with_capacity(64),
                Vec::with_capacity(64),
                Vec::with_capacity(64),
                Vec::with_capacity(64),
            ],
        }
    }

    pub fn insert(&mut self, chunk_pos: (i32, i32)) {
        let color = chunk_domain_color(chunk_pos.0, chunk_pos.1);
        self.domains[color].push(chunk_pos);
    }

    pub fn chunks_in_color(&self, color: usize) -> &[(i32, i32)] {
        &self.domains[color]
    }

    pub fn clear(&mut self) {
        for domain in &mut self.domains {
            domain.clear();
        }
    }
}
