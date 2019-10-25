//! # Quickdry
//!
//! Quickdry is a library for arena allocation.
//!
//! An arena keeps a single pointer into a large slab of memory. Allocation is very quick, because
//! it simply bumps the pointer forward, and maintains no metadata about individual allocations. In
//! exchange, all the memory in an arena must be freed at once.
//!
//! For example, a circular doubly-linked list in the style of the Linux kernel's:
//!
//! ```rust
//! use std::{ptr, cell::Cell};
//! use std::alloc::Layout;
//! use quickdry::Arena;
//!
//! pub struct Node<'a> {
//!     pub data: i32,
//!     pub next: Cell<&'a Node<'a>>,
//!     pub prev: Cell<&'a Node<'a>>,
//! }
//!
//! impl<'a> Node<'a> {
//!     pub fn in_arena(arena: &'a Arena, data: i32) -> &'a Self {
//!         unsafe {
//!             let ptr = arena.alloc(Layout::new::<Self>()) as *mut Self;
//!             let next = Cell::new(&*ptr);
//!             let prev = Cell::new(&*ptr);
//!             ptr::write(ptr, Node { data, next, prev });
//!             &*ptr
//!         }
//!     }
//!
//!     pub fn add_head(&'a self, new: &'a Self) { Self::insert(new, self, self.next.get()) }
//!     pub fn add_tail(&'a self, new: &'a Self) { Self::insert(new, self.prev.get(), self) }
//!     pub fn del(&'a self) { Self::remove(self.prev.get(), self.next.get()) }
//!
//!     fn insert(new: &'a Self, prev: &'a Self, next: &'a Self) {
//!         next.prev.set(new);
//!         new.next.set(next);
//!         new.prev.set(prev);
//!         prev.next.set(new);
//!     }
//!
//!     fn remove(prev: &'a Self, next: &'a Self) {
//!         next.prev.set(prev);
//!         prev.next.set(next);
//!     }
//! }
//!
//! fn main() {
//!     let arena = Arena::default();
//!     
//!     let list = Node::in_arena(&arena, 3);
//!     list.add_head(Node::in_arena(&arena, 5));
//!     list.add_tail(Node::in_arena(&arena, 8));
//!
//!     assert_eq!(list.data, 3);
//!     assert_eq!(list.next.get().data, 5);
//!     assert_eq!(list.next.get().next.get().data, 8);
//!     assert_eq!(list.next.get().next.get().next.get().data, 3);
//!
//!     list.next.get().del();
//!
//!     assert_eq!(list.data, 3);
//!     assert_eq!(list.next.get().data, 8);
//!     assert_eq!(list.next.get().next.get().data, 3);
//! }
//! ```

use std::{ptr, slice, cmp};
use std::alloc::{alloc, Layout};
use std::cell::{Cell, UnsafeCell};

/// Bump-pointer allocator.
pub struct Arena {
    slabs: UnsafeCell<Vec<Box<[u8]>>>,
    next: Cell<*mut u8>,
    end: Cell<*mut u8>,
}

const SLAB_SIZE: usize = 0x1000;

impl Default for Arena {
    fn default() -> Self {
        let slabs = UnsafeCell::new(Vec::default());
        let next = Cell::new(ptr::null_mut());
        let end = Cell::new(ptr::null_mut());
        Arena { slabs, next, end }
    }
}

impl Arena {
    /// Allocate memory via bump-pointer.
    ///
    /// # Safety
    ///
    /// See `std::alloc::alloc`.
    pub unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // Try to fit the allocation into the current slab.
        let next = self.next.get();
        let end = self.end.get();
        let offset = align_offset(next as usize, layout.align());
        if offset + layout.size() <= end as usize - next as usize {
            let ptr = next.add(offset);
            self.next.set(ptr.add(layout.size()));
            return ptr;
        }

        // If the allocation is big enough, use a one-off slab.
        // This is similar to `alloc_new_slab`, but leaves `next` and `end` in place.
        let padded = layout.size() + layout.align() - 1;
        if padded > SLAB_SIZE {
            let slabs = &mut *self.slabs.get();

            let next = alloc(Layout::from_size_align_unchecked(padded, 1));
            if next == ptr::null_mut() {
                return ptr::null_mut();
            }

            let slab = Box::from_raw(slice::from_raw_parts_mut(next, padded));
            slabs.push(slab);

            let ptr = align_to(next as usize, layout.align()) as *mut u8;
            return ptr;
        }

        // Otherwise, we need to start a new slab.
        self.alloc_new_slab(layout)
    }

    unsafe fn alloc_new_slab(&self, layout: Layout) -> *mut u8 {
        let slabs = &mut *self.slabs.get();

        // Double slab sizes every 128 slabs until `i32::MAX`.
        let size = SLAB_SIZE * (1 << cmp::min(30, slabs.len() / 128));
        let next = alloc(Layout::from_size_align_unchecked(size, 1));
        if next == ptr::null_mut() {
            return ptr::null_mut();
        }

        // Save the slab and its size for drop.
        let slab = Box::from_raw(slice::from_raw_parts_mut(next, size));
        slabs.push(slab);
        self.next.set(next);
        self.end.set(next.add(size));

        let ptr = align_to(next as usize, layout.align()) as *mut u8;
        self.next.set(ptr.add(layout.size()));
        ptr
    }
}

/// The offset needed to align `size` to `align`.
fn align_offset(size: usize, align: usize) -> usize {
    align_to(size, align) - size
}

/// Align `size` to `align`.
fn align_to(size: usize, align: usize) -> usize {
    (size + align - 1) & !(align - 1)
}
