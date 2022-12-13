//! # Soak
//!
//! Soak is a library for transforming data from an Array-of-Structs layout to a
//! Struct-of-Arrays layout.
//!
//! The natural way to work with a collection of objects in Rust is to represent them as structs,
//! often placed in a [`Vec`]. This lays out each individual object's fields together in memory:
//!
//! ```text
//! [field 1, field 2][field 1, field 2][field 1, field 2]...
//! ```
//!
//! Often it improves performance to interleave the objects' fields, so that individual fields are
//! grouped together instead:
//!
//! ```text
//! [field 1, field 1, field 1][field 2, field 2, field 2]...
//! ```
//!
//! The primary tools provided by Soak are the [`Columns`] trait, which records a struct's layout;
//! and the [`RawTable`] type, the eponymous struct of arrays. They can be used together like this:
//!
//! ```no_run
//! use core::ptr;
//! use dioptre::Fields;
//! use soak::{RawTable, Columns};
//!
//! #[derive(Fields, Columns)]
//! struct GameObject {
//!     position: (f32, f32),
//!     velocity: (f32, f32),
//!     health: f32,
//! }
//!
//! unsafe fn process(table: &mut RawTable<GameObject>) {
//!     let positions = table.ptr(GameObject::position);
//!     let velocities = table.ptr(GameObject::velocity);
//!     let healths = table.ptr(GameObject::health);
//!
//!     for i in 0..table.capacity() {
//!         let position = &mut *positions.add(i);
//!         let velocity = &mut *velocities.add(i);
//!         position.0 += velocity.0;
//!         position.1 += velocity.1;
//!     }
//! }
//! ```

#![no_std]

extern crate alloc;

use core::{mem, ptr, usize};
use core::borrow::{Borrow, BorrowMut};
use core::marker::PhantomData;
use alloc::alloc::{alloc, dealloc, handle_alloc_error, Layout};
use dioptre::{Fields, Field};

pub use soak_derive::Columns;

/// Metadata required to use a struct in a [`RawTable`].
///
/// This trait should not normally be implemented by hand. Instead, use `#[derive(Columns)]`- this
/// will safely generate the appropriate trait impl.
///
/// # Safety
///
/// * `Pointers` must be a fixed-size array matching `Fields::SIZES` and `Fields::ALIGNS` in length.
/// * `dangling()` must contain `ptr::NonNull::dangling()`.
pub unsafe trait Columns: Fields {
    /// A fixed-size array of pointers to field arrays.
    type Pointers: BorrowMut<[ptr::NonNull<u8>]>;
    /// An empty value for `Self::Pointers`.
    fn dangling() -> Self::Pointers;
}

/// A raw allocation containing parallel arrays of `T`'s fields.
///
/// Much like `std`'s `RawVec`, `RawTable` manages an allocation for a collection, but without
/// managing the initialization or dropping of its contents. `RawTable` does not deal directly with
/// elements of type `T`, but with multiple adjacent arrays of `T`'s fields, shared in a single
/// allocation.
pub struct RawTable<T: Columns> {
    pointers: T::Pointers,
    capacity: usize,
    _marker: PhantomData<T>,
}

impl<T: Columns> Default for RawTable<T> {
    /// Create a `RawTable` without allocating.
    fn default() -> Self {
        let pointers = T::dangling();
        let capacity = if mem::size_of::<T>() == 0 { usize::MAX } else { 0 };
        RawTable { pointers, capacity, _marker: PhantomData }
    }
}

impl<T: Columns> RawTable<T> {
    /// Create a `RawTable` with enough space for `capacity` elements of each field type.
    ///
    /// # Panics
    ///
    /// Panics if the requested capacity exceeds [`usize::MAX`] bytes.
    ///
    /// # Aborts
    ///
    /// Aborts on OOM.
    pub fn with_capacity(capacity: usize) -> Self {
        unsafe {
            let align = T::ALIGNS.iter().cloned().max().unwrap_or(1);
            let mask = align - 1;
            let size = T::SIZES.iter().try_fold(0, move |sum, &size| {
                let array_size = usize::checked_mul(capacity, size)?;
                let aligned_size = usize::checked_add(array_size, mask)? & !mask;
                Some(usize::checked_add(sum, aligned_size)?)
            }).expect("capacity overflow");

            let layout = Layout::from_size_align_unchecked(size, align);
            let data = if size == 0 { align as *mut u8 } else { alloc(layout) };
            if data == ptr::null_mut() {
                handle_alloc_error(layout);
            }

            let mut pointers = T::dangling();
            let mut offset = 0;
            let dst = pointers.borrow_mut().iter_mut();
            for (pointer, size) in Iterator::zip(dst, T::SIZES.iter()) {
                *pointer = ptr::NonNull::new_unchecked(data.add(offset));
                offset += (capacity * size + mask) & !mask;
            }

            let capacity = if mem::size_of::<T>() == 0 { usize::MAX } else { capacity };

            RawTable { pointers, capacity, _marker: PhantomData }
        }
    }

    /// Get a pointer to a field array.
    pub fn ptr<F>(&mut self, field: Field<T, F>) -> *mut F {
        self.pointers.borrow()[field.index()].as_ptr() as *mut F
    }

    /// Get the capacity of the allocation.
    pub fn capacity(&self) -> usize { self.capacity }

    /// Ensure that the table contains enough space for `used + extra` elements.
    ///
    /// # Panics
    ///
    /// Panics if the requested capacity exceeds [`usize::MAX`] bytes.
    ///
    /// # Aborts
    ///
    /// Aborts on OOM.
    pub fn reserve_exact(&mut self, used: usize, extra: usize) {
        unsafe {
            if self.capacity - used >= extra {
                return;
            }

            let capacity = usize::checked_add(used, extra).expect("capacity overflow");
            let table = Self::with_capacity(capacity);

            let src = self.pointers.borrow().iter();
            let dst = table.pointers.borrow().iter();
            for ((src, dst), size) in Iterator::zip(Iterator::zip(src, dst), T::SIZES.iter()) {
                ptr::copy_nonoverlapping(src.as_ptr(), dst.as_ptr(), self.capacity * size);
            }

            let _ = mem::replace(self, table);
        }
    }
}

impl<T: Columns> Drop for RawTable<T> {
    /// Free the underlying buffer but do not drop the arrays' elements.
    fn drop(&mut self) {
        unsafe {
            let align = *T::ALIGNS.iter().max().unwrap_or(&1);
            let mask = align - 1;

            let capacity = self.capacity;
            let size = T::SIZES.iter().map(move |&size| (capacity * size + mask) & !mask).sum();

            let layout = Layout::from_size_align_unchecked(size, align);
            if size > 0 { dealloc(self.pointers.borrow()[0].as_ptr(), layout); }
        }
    }
}
