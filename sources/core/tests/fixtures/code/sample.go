// Package orders holds orders.
package orders

import "fmt"

// Most is the most an order holds.
const Most = 10

const (
	Open = iota
	Shut
)

var count = 0

// Order is an order.
type Order struct {
	lines []int
}

// Total adds the lines up.
func (o *Order) Total() int {
	local := func() {}
	local()
	return sum(o.lines) + helper(1)
}

func (o Order) String() string {
	return fmt.Sprintf("%d", o.Total())
}

func (g *Ghost) Appear() {}

func (g Ghost) Vanish() {}

type Priced interface {
	Price() int
}

type (
	Lines []int
	State int
)

// A plain comment, then a blank line: not the function's.

// helper helps.
func helper(n int) int {
	return n + 1
}

func sum[T int | float64](xs []T) T {
	var s T
	return s
}
