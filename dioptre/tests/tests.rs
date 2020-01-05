use core::cell::Cell;
use dioptre::{Fields, ext::CellExt};

#[derive(Fields)]
struct Data {
    x: i32,
    y: i32,
}

#[test]
fn project() {
    let mut data = Data { x: 3, y: 5 };

    let rc = Cell::from_mut(&mut data);
    rc.project(Data::x).set(8);
    rc.project(Data::y).set(13);

    assert_eq!(data.x, 8);
    assert_eq!(data.y, 13);
}
