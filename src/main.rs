extern crate conway;

use std::{thread, time};
use conway::*;

fn main() {
    let mut uni = Universe::new(128, 32, true, 16, 2, vec![Region::new(40,6,16,8), Region::new(60,16,8,8)]).unwrap();
    let step_time = time::Duration::from_millis(150);

    // pi heptomino
    uni.toggle(62, 17, 1).unwrap();
    uni.toggle(63, 17, 1).unwrap();
    uni.toggle(61, 18, 1).unwrap();
    uni.toggle(62, 18, 1).unwrap();
    uni.toggle(62, 19, 1).unwrap();

    // Spaceship in reverse direction
    uni.toggle(48, 6, 0).unwrap();
    uni.toggle(47, 7, 0).unwrap();
    uni.toggle(47, 8, 0).unwrap();
    uni.toggle(52, 8, 0).unwrap();
    uni.toggle(47, 9, 0).unwrap();
    uni.toggle(48, 9, 0).unwrap();
    uni.toggle(49, 9, 0).unwrap();
    uni.toggle(50, 9, 0).unwrap();
    uni.toggle(51, 9, 0).unwrap();

    uni.set(74, 13, CellState::Wall);
    uni.set(75, 13, CellState::Wall);
    uni.set(76, 13, CellState::Wall);
    uni.set(77, 13, CellState::Wall);
    uni.set(78, 13, CellState::Wall);
    uni.set(78, 14, CellState::Wall);
    uni.set(78, 15, CellState::Wall);
    uni.set(78, 16, CellState::Wall);
    uni.set(78, 17, CellState::Wall);
    uni.set(78, 18, CellState::Wall);
    uni.set(78, 19, CellState::Wall);
    uni.set(78, 20, CellState::Wall);
    uni.set(78, 21, CellState::Wall);
    uni.set(78, 22, CellState::Wall);
    uni.set(78, 23, CellState::Wall);
    uni.set(78, 24, CellState::Wall);
    uni.set(77, 24, CellState::Wall);
    uni.set(76, 24, CellState::Wall);
    uni.set(75, 24, CellState::Wall);

    loop {
        println!("\x1b[H\x1b[2J{}", uni);
        println!("Gen: {}", uni.latest_gen());
        uni.next();
        thread::sleep(step_time);
    }
}
