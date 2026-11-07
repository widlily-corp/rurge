use rurge_client::hardware::{compute_cpu_budget, compute_ram_budget, RamTier};

#[test]
fn test_cpu_allocation_formula() {
    // 4 cores -> 3 for game, 1 for OS
    let b4 = compute_cpu_budget(4, 8);
    assert_eq!(b4.game_physical_cores, 3);
    assert_eq!(b4.os_reserved_cores, 1);
    assert_eq!(b4.worker_threads, 2);

    // 8 cores -> 6 for game, 2 for OS
    let b8 = compute_cpu_budget(8, 16);
    assert_eq!(b8.game_physical_cores, 6);
    assert_eq!(b8.os_reserved_cores, 2);
    assert_eq!(b8.worker_threads, 5);

    // 16 cores -> 13 for game, 3 for OS
    let b16 = compute_cpu_budget(16, 32);
    assert_eq!(b16.game_physical_cores, 13);
    assert_eq!(b16.os_reserved_cores, 3);
}

#[test]
fn test_ram_tier_classification() {
    const GIB: u64 = 1024 * 1024 * 1024;
    let b_8gb = compute_ram_budget(8 * GIB);
    assert_eq!(b_8gb.tier, RamTier::Lean);
    assert!(b_8gb.max_game_bytes <= (2.5 * GIB as f64) as u64);

    let b_16gb = compute_ram_budget(16 * GIB);
    assert_eq!(b_16gb.tier, RamTier::Standard);
    assert!(b_16gb.max_game_bytes <= (4.5 * GIB as f64) as u64);

    let b_32gb = compute_ram_budget(32 * GIB);
    assert_eq!(b_32gb.tier, RamTier::HighEnd);
    assert_eq!(b_32gb.max_game_bytes, 8 * GIB);
}
