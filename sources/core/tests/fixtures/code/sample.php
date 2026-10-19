<?php

namespace Shop;

/** The most an order holds. */
const MOST = 10;

/** An order. */
#[Entity]
final class Order implements Priced
{
    public const LEAST = 1;

    private array $lines = [];

    /** A new one. */
    public function __construct()
    {
        $this->lines = [];
    }

    public function total(): int
    {
        return array_sum($this->lines) + helper(1);
    }

    public static function of(array $lines): self
    {
        return new Order();
    }
}

interface Priced
{
    public function price(): int;
}

trait Counted
{
    public function count(): int { return 0; }
}

enum State
{
    case Open;
    case Shut;
}

// A plain comment, then a blank line: not the function's.

/** Helps. */
function helper(int $n): int
{
    $f = fn($x) => $x + 1;
    return $f($n);
}
