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
//! use core::{ptr, cell::Cell};
//! use core::alloc::Layout;
//! use quickdry::Arena;
//!
//! #[repr(C)]
//! pub struct Node<'a> {
//!     pub data: i32,
//!     pub next: Cell<&'a Node<'a>>,
//!     pub prev: Cell<&'a Node<'a>>,
//! }
//!
//! impl<'a> Node<'a> {
//!     pub fn in_arena(arena: &'a Arena, data: i32) -> &'a Self {
//!         #[repr(C)]
//!         struct NodeUninit<'a> {
//!             data: i32,
//!             next: Cell<Option<&'a NodeUninit<'a>>>,
//!             prev: Cell<Option<&'a NodeUninit<'a>>>,
//!         }
//!
//!         unsafe {
//!             let ptr = arena.alloc(Layout::new::<Self>()) as *mut Self;
//!
//!             let bootstrap = ptr as *mut NodeUninit<'a>;
//!             let next = Cell::new(None);
//!             let prev = Cell::new(None);
//!             ptr::write(bootstrap, NodeUninit { data, next, prev });
//!
//!             let node = &*bootstrap;
//!             node.next.set(Some(node));
//!             node.prev.set(Some(node));
//!
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

#![no_std]

extern crate alloc;

use core::{ptr, cmp};
use core::cell::{Cell, UnsafeCell};
use alloc::alloc::{alloc, Layout};
use alloc::{boxed::Box, vec::Vec};

/// Bump-pointer allocator.
pub struct Arena {
    slabs: UnsafeCell<Vec<Box<[u8]>>>,
    next: Cell<*mut u8>,
    end: Cell<*mut u8>,
}

const SLAB_SIZE: usize = 0x1000;

impl Default for Arena {
    #[inline]
    fn default() -> Self {
        let slabs = UnsafeCell::new(Vec::default());
        let next = Cell::new(ptr::dangling_mut());
        let end = Cell::new(ptr::dangling_mut());
        Arena { slabs, next, end }
    }
}

impl Arena {
    /// Allocate memory via bump-pointer.
    ///
    /// # Safety
    ///
    /// See `std::alloc::alloc`.
    #[inline]
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

        // Otherwise, we need to start a new slab.
        self.alloc_new_slab(layout)
    }

    #[cold]
    unsafe fn alloc_new_slab(&self, layout: Layout) -> *mut u8 {
        // If the allocation is big enough, use a one-off slab.
        let padded = layout.size() + layout.align() - 1;
        if padded > SLAB_SIZE {
            let next = self.alloc_slab(padded);
            if next == ptr::null_mut() { return ptr::null_mut(); }

            let offset = align_offset(next as usize, layout.align());
            return next.add(offset);
        }

        // Double slab sizes every 128 slabs until `i32::MAX`.
        let slabs = &*self.slabs.get();
        let size = SLAB_SIZE * (1 << cmp::min(30, slabs.len() / 128));

        let next = self.alloc_slab(size);
        if next == ptr::null_mut() { return ptr::null_mut(); }

        let offset = align_offset(next as usize, layout.align());
        let ptr = next.add(offset);

        self.next.set(ptr.add(layout.size()));
        self.end.set(next.add(size));

        ptr
    }

    unsafe fn alloc_slab(&self, size: usize) -> *mut u8 {
        let slabs = &mut *self.slabs.get();

        let next = alloc(Layout::from_size_align_unchecked(size, 1));
        if next == ptr::null_mut() { return ptr::null_mut(); }

        // Save the slab and its size for drop.
        let slab = Box::from_raw(ptr::slice_from_raw_parts_mut(next, size));
        slabs.push(slab);

        // Reborrow the slab from the box's new (and final) location.
        slabs.last_mut().unwrap().as_mut_ptr()
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
