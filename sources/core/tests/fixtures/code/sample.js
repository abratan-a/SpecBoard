import fs from "fs";

/** The most an order holds. */
export const MOST = 10;

/** An order. */
export class Order {
  /** A new one. */
  constructor() {
    function local() {}
    local();
    this.lines = [];
  }

  total() {
    return this.lines.reduce((a, b) => a + b, 0) + helper(1);
  }

  static of(lines) {
    return new Order();
  }
}

// A plain comment, then a blank line: not the function's.

/** Helps. */
const helper = (n) => {
  const f = (x) => x + 1;
  return f(n);
};

let late = function () {};

function* gen() {
  yield 1;
}

module.exports = { helper };
