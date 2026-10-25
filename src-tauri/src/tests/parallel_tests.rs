#[test]
fn par_map_keeps_index_order_and_covers_every_item() {
    for n in [0usize, 1, 2, 3, 7, 64, 1000] {
        let got = crate::parallel::par_map_indexed(n, |i| i * 3 + 1);
        let want: Vec<usize> = (0..n).map(|i| i * 3 + 1).collect();
        assert_eq!(got, want, "n = {n}");
    }
    let words = ["a".to_string(), "bb".to_string(), "ccc".to_string()];
    assert_eq!(crate::parallel::par_map(&words, |w| w.len()), vec![1, 2, 3]);
}

#[test]
fn par_map_balances_uneven_work() {
    // Triangular cost, like the duplicate scan: later indices are cheap, earlier ones are not.
    let n = 200usize;
    let got = crate::parallel::par_map_indexed(n, |i| (i..n).map(|j| (j * i) as u64).sum::<u64>());
    for (i, v) in got.iter().enumerate() {
        let want: u64 = (i..n).map(|j| (j * i) as u64).sum();
        assert_eq!(*v, want);
    }
}

#[test]
fn par_slabs_touch_every_element_exactly_once_in_whole_units() {
    // Large enough to be split across threads on any machine with more than one core.
    let mut data = vec![0u8; (3 << 20) + 8];
    crate::parallel::par_slabs_mut(&mut data, 4, |slab| {
        assert_eq!(slab.len() % 4, 0, "a slab never cuts a 4-byte pixel");
        for b in slab.iter_mut() {
            *b += 1;
        }
    });
    assert!(data.iter().all(|&b| b == 1));
    let mut tiny = [0u8; 8];
    crate::parallel::par_slabs_mut(&mut tiny, 4, |s| s.fill(9));
    assert_eq!(tiny, [9; 8]);
    let mut none: [u8; 0] = [];
    crate::parallel::par_slabs_mut(&mut none, 4, |_| {});
}

#[test]
#[should_panic(expected = "boom")]
fn a_panic_inside_a_parallel_map_reaches_the_caller() {
    let _ = crate::parallel::par_map_indexed(50, |i| if i == 33 { panic!("boom") } else { i });
}

#[test]
fn the_pixel_swap_turns_bgra_into_opaque_rgba() {
    let mut px = vec![10u8, 20, 30, 0, 1, 2, 3, 77];
    crate::snip::bgra_to_opaque_rgba(&mut px);
    assert_eq!(px, vec![30, 20, 10, 255, 3, 2, 1, 255]);
    // A buffer that is split across threads gives the same answer as the plain loop.
    let n = (5 << 20) + 4;
    let mut big: Vec<u8> = (0..n).map(|i| (i % 253) as u8).collect();
    let mut want = big.clone();
    for p in want.as_chunks_mut::<4>().0 {
        p.swap(0, 2);
        p[3] = 255;
    }
    crate::snip::bgra_to_opaque_rgba(&mut big);
    assert!(big == want);
}
