use core::{ptr, slice};
use dioptre::Fields;
use soak::{Columns, RawTable};

#[derive(Copy, Clone, Fields, Columns)]
#[allow(dead_code)]
struct Data {
    x: u8,
    y: u32,
    z: u64,
}

#[test]
fn layout() {
    let mut table: RawTable<Data> = RawTable::with_capacity(64);

    unsafe {
        let x = table.ptr(Data::x);
        let y = table.ptr(Data::y);
        let z = table.ptr(Data::z);
        assert_eq!(x as usize & (1 - 1), 0);
        assert_eq!(y as usize & (4 - 1), 0);
        assert_eq!(z as usize & (8 - 1), 0);
        for i in 0..64 {
            ptr::write(x.offset(i as isize), i as u8);
            ptr::write(y.offset(i as isize), 64 + i as u32);
            ptr::write(z.offset(i as isize), 128 + i as u64);
        }
    }

    table.reserve_exact(64, 64);

    unsafe {
        let x = slice::from_raw_parts(table.ptr(Data::x), 64);
        let y = slice::from_raw_parts(table.ptr(Data::y), 64);
        let z = slice::from_raw_parts(table.ptr(Data::z), 64);
        let iter = Iterator::zip(Iterator::zip(x.iter(), y.iter()), z.iter()).enumerate();
        for (i, ((&x, &y), &z)) in iter {
            assert_eq!(x as usize, i);
            assert_eq!(y as usize, 64 + i);
            assert_eq!(z as usize, 128 + i);
        }
    }
}
