//! A module's own doc, not a symbol's.

use std::fmt;

/// The most an order holds.
pub const MOST: usize = 10;
static mut COUNT: u32 = 0;

/// An order.
#[derive(Clone, Debug)]
pub struct Order {
    lines: Vec<u32>,
}

impl Order {
    /// A new one.
    pub fn new() -> Order {
        fn local() {}
        local();
        Order { lines: Vec::new() }
    }

    pub fn total(&self) -> u32 {
        self.lines.iter().sum::<u32>() + helper(1)
    }
}

impl fmt::Display for Order {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.total())
    }
}

impl fmt::Debug for Order {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Ok(())
    }
}

impl fmt::Debug for Ghost {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Ok(())
    }
}

impl Ghost {
    fn appear(&self) {}
}

pub enum State {
    Open,
    Shut,
}

pub trait Priced {
    fn price(&self) -> u32;
    fn cheap(&self) -> bool {
        self.price() < 10
    }
}

type Lines = Vec<u32>;

macro_rules! twice {
    ($e:expr) => {
        $e * 2
    };
}

mod inner {
    pub struct Deep;

    impl Deep {
        pub fn go() {}
    }

    pub fn helper2() {}
}

// A plain comment, then a blank line: not the function's.

/// Helps.
fn helper(n: u32) -> u32 {
    let f = |x: u32| x + 1;
    twice!(f(n))
}

extern "C" {
    fn abs(x: i32) -> i32;
}
