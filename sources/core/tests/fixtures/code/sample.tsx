import { x } from "./x";

/** The most an order holds. */
export const MOST = 10;

/** An order. */
@sealed
export class Order implements Priced {
  private lines: number[] = [];

  /** A new one. */
  constructor() {
    function local() {}
    local();
  }

  total(): number {
    return this.lines.reduce((a, b) => a + b, 0) + helper(1);
  }

  get count(): number {
    return this.lines.length;
  }
}

export interface Priced {
  price(): number;
  cheap(): boolean;
}

enum State {
  Open,
  Shut,
}

type Lines = number[];

namespace Inner {
  export function deep() {}
}

export default function main() {
  console.log(new Order().total());
}

// A plain comment, then a blank line: not the function's.

/** Helps. */
const helper = (n: number): number => {
  const f = (x: number) => x + 1;
  return f(n);
};

function over(a: string): void;
function over(a: number): void;
function over(a: any) {}

abstract class Shape {
  abstract area(): number;
}

export function View() {
  return <div onClick={() => helper(1)}>{MOST}</div>;
}
