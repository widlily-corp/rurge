//! Hardware profiler and adaptive CPU/RAM resource manager.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RamTier {
    Lean,
    Standard,
    HighEnd,
}

#[derive(Debug, Clone, Copy)]
pub struct CpuBudget {
    pub total_physical_cores: usize,
    pub total_logical_threads: usize,
    pub game_physical_cores: usize,
    pub os_reserved_cores: usize,
    pub worker_threads: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct MemoryBudget {
    pub total_system_bytes: u64,
    pub max_game_bytes: u64,
    pub tier: RamTier,
    pub region_cache_in_ram: bool,
    pub max_chunk_mesh_radius: u32,
}

/// Computes the CPU core allocation, guaranteeing OS headroom (Discord, OS, background apps).
pub fn compute_cpu_budget(phys: usize, log: usize) -> CpuBudget {
    let phys = phys.max(1);
    let os_cores = if phys <= 4 {
        1
    } else if phys <= 8 {
        2
    } else {
        (phys / 5).max(3)
    };
    let game_cores = phys.saturating_sub(os_cores).max(1);
    let worker_threads = if game_cores <= 2 { 1 } else { game_cores - 1 };

    CpuBudget {
        total_physical_cores: phys,
        total_logical_threads: log,
        game_physical_cores: game_cores,
        os_reserved_cores: os_cores,
        worker_threads,
    }
}

/// Computes the RAM budget according to system RAM capacity, preventing OS swapping.
pub fn compute_ram_budget(total_bytes: u64) -> MemoryBudget {
    const GIB: u64 = 1024 * 1024 * 1024;
    let gib = total_bytes / GIB;

    if gib < 12 {
        MemoryBudget {
            total_system_bytes: total_bytes,
            max_game_bytes: (2.5 * GIB as f64) as u64,
            tier: RamTier::Lean,
            region_cache_in_ram: false,
            max_chunk_mesh_radius: 12,
        }
    } else if gib < 24 {
        MemoryBudget {
            total_system_bytes: total_bytes,
            max_game_bytes: (4.5 * GIB as f64) as u64,
            tier: RamTier::Standard,
            region_cache_in_ram: false,
            max_chunk_mesh_radius: 20,
        }
    } else {
        MemoryBudget {
            total_system_bytes: total_bytes,
            max_game_bytes: 8 * GIB,
            tier: RamTier::HighEnd,
            region_cache_in_ram: true,
            max_chunk_mesh_radius: 32,
        }
    }
}

/// Queries the host system directly and produces the hardware profile.
pub fn detect_hardware() -> (CpuBudget, MemoryBudget) {
    let phys = num_cpus::get_physical();
    let log = num_cpus::get();
    let mut sys = sysinfo::System::new_all();
    sys.refresh_memory();
    let total_ram = sys.total_memory();

    (compute_cpu_budget(phys, log), compute_ram_budget(total_ram))
}
