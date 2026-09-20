//! Tests that hold this repository's patches to the vendored C **from the C's
//! own entry points**.
//!
//! `sys.rs` binds the public API this client calls. What is declared here is
//! internal to toxcore -- `list.h` -- and is in every build only because the
//! library is linked statically; it is declared for the tests below and for
//! nothing else. The rule of `sys.rs` holds here too: no signature is guessed,
//! each is copied from the header and carries its line.
//!
//! Most of `patches/` is held by a measurement on the bed, because most of it
//! is about time and the network. A patch about arithmetic can be held by a
//! test, so it is.

use std::ffi::{c_int, c_void};

/// `list.h:25`
type BsListCmp = unsafe extern "C" fn(a: *const c_void, b: *const c_void, size: usize) -> c_int;

/// `list.h:27`
#[repr(C)]
struct BsList {
    mem: *const c_void,
    n: u32,
    capacity: u32,
    element_size: u32,
    data: *mut u8,
    ids: *mut c_int,
    cmp_callback: Option<BsListCmp>,
}

extern "C" {
    /// `os_memory.h:16`
    fn os_memory() -> *const c_void;
    /// `list.h:46`
    fn bs_list_init(
        list: *mut BsList,
        mem: *const c_void,
        element_size: u32,
        initial_capacity: u32,
        cmp_callback: BsListCmp,
    ) -> c_int;
    /// `list.h:49`
    fn bs_list_free(list: *mut BsList);
    /// `list.h:55`
    fn bs_list_find(list: *const BsList, data: *const u8) -> c_int;
    /// `list.h:62`
    fn bs_list_add(list: *mut BsList, data: *const u8, id: c_int) -> bool;
    /// `list.h:69`
    fn bs_list_remove(list: *mut BsList, data: *const u8, id: c_int) -> bool;
}

/// `memcmp`, which is what toxcore gives its own lists of public keys.
unsafe extern "C" fn compare(a: *const c_void, b: *const c_void, size: usize) -> c_int {
    let (a, b) = (std::slice::from_raw_parts(a.cast::<u8>(), size), std::slice::from_raw_parts(b.cast::<u8>(), size));
    match a.cmp(b) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    }
}

const KEY: usize = 32;

/// A key no two indices share, in no order a sorted list would like.
fn key(j: u32) -> [u8; KEY] {
    let mut k = [0u8; KEY];
    let mut x = u64::from(j).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03;
    for chunk in k.chunks_mut(8) {
        x ^= x >> 29;
        x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        chunk.copy_from_slice(&x.to_be_bytes());
    }
    k[KEY - 4..].copy_from_slice(&j.to_be_bytes());
    k
}

/// Patch 0041: **a list whose bytes do not fit the allocator's 32 bits is
/// refused.** Four elements of one gibibyte and one byte are 2^32 + 4 bytes,
/// which two `uint32_t` multiplied in 32 bits make **4**: the list came back
/// holding room for four such elements in a buffer of four bytes, and the first
/// `bs_list_add` would have copied a gibibyte into it. Nothing is added here --
/// the refusal is the whole of what is asked.
///
/// The break that must make this fail: `resize()` allocating with
/// `mem_brealloc(..., new_size * list->element_size)` again.
#[test]
fn a_list_whose_bytes_do_not_fit_is_refused() {
    let mut list = std::mem::MaybeUninit::<BsList>::zeroed();
    let made = unsafe { bs_list_init(list.as_mut_ptr(), os_memory(), (1 << 30) + 1, 4, compare) };
    let mut list = unsafe { list.assume_init() };
    let (capacity, held) = (list.capacity, !list.data.is_null());
    unsafe { bs_list_free(&mut list) };
    assert_eq!(made, 0, "2^32 + 4 bytes are not 4 bytes");
    assert_eq!((capacity, held), (0, false), "and a list that was refused holds nothing");
}

/// And a list that does fit is the list it always was: what goes in is found
/// under its id, in whatever order it went in, through every growth of the
/// arrays and every shrinking -- the offsets and lengths patch 0041 widened.
///
/// The break that must make this fail: any of the three `memmove`s moving from
/// or to the wrong element.
#[test]
fn a_list_that_fits_still_finds_what_it_was_given() {
    const N: u32 = 600;
    let mut list = std::mem::MaybeUninit::<BsList>::zeroed();
    assert_eq!(unsafe { bs_list_init(list.as_mut_ptr(), os_memory(), KEY as u32, 0, compare) }, 1);
    let mut list = unsafe { list.assume_init() };
    for j in 0..N {
        assert!(unsafe { bs_list_add(&mut list, key(j).as_ptr(), j as c_int) }, "key {j} goes in");
    }
    assert_eq!(list.n, N);
    assert!(!unsafe { bs_list_add(&mut list, key(7).as_ptr(), 9_999) }, "a key is held once");
    for j in 0..N {
        assert_eq!(unsafe { bs_list_find(&list, key(j).as_ptr()) }, j as c_int, "key {j} is found under its id");
    }
    // The elements are in order, which is what the search stands on.
    let bytes = unsafe { std::slice::from_raw_parts(list.data, N as usize * KEY) };
    assert!(bytes.chunks(KEY).zip(bytes.chunks(KEY).skip(1)).all(|(a, b)| a < b), "sorted, no two alike");

    assert!(!unsafe { bs_list_remove(&mut list, key(3).as_ptr(), 4) }, "not under another key's id");
    for j in (0..N).filter(|j| j % 3 != 0) {
        assert!(unsafe { bs_list_remove(&mut list, key(j).as_ptr(), j as c_int) }, "key {j} comes out");
    }
    assert_eq!(list.n, N / 3);
    assert!(list.capacity < N, "and the arrays shrank behind it: {} for {}", list.capacity, list.n);
    for j in 0..N {
        let want = if j % 3 == 0 { j as c_int } else { -1 };
        assert_eq!(unsafe { bs_list_find(&list, key(j).as_ptr()) }, want, "key {j} after the removals");
    }
    unsafe { bs_list_free(&mut list) };
}
