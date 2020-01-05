use std::cell::Cell;
use crate::{Fields, Field};

/// Additional methods for `Cell`.
pub trait CellExt {
    type Inner;

    /// Access a field of a `Cell`.
    ///
    /// ```no_run
    /// use core::cell::Cell;
    /// use dioptre::{Fields, ext::CellExt};
    ///
    /// #[derive(Fields)]
    /// struct Data {
    ///     x: i32,
    ///     y: i32,
    /// }
    ///
    /// fn process(data: &Cell<Data>) {
    ///     data.project(Data::x).set(3);
    ///     data.project(Data::y).set(5);
    /// }
    /// ```
    fn project<T>(&self, field: Field<Self::Inner, T>) -> &Cell<T>;
}

impl<S: Fields> CellExt for Cell<S> {
    type Inner = S;

    fn project<T>(&self, field: Field<S, T>) -> &Cell<T> {
        unsafe {
            let offset = S::OFFSETS[field.index()](self.as_ptr() as *mut _);
            let field = (self as *const _ as *const u8).add(offset);
            &*(field as *const Cell<T>)
        }
    }
}
