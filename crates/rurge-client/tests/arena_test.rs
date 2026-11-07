use rurge_client::memory::arena::TickArena;

#[test]
fn test_tick_arena_alloc_and_reset() {
    let mut arena = TickArena::with_capacity(1024 * 1024);
    let slice = arena.alloc_slice_copy(&[1u32, 2, 3, 4]);
    assert_eq!(slice, &[1, 2, 3, 4]);

    arena.reset();
    let slice2 = arena.alloc_slice_copy(&[5u32, 6]);
    assert_eq!(slice2, &[5, 6]);
}
