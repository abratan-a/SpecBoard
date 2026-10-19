package shop

import kotlin.math.max

/** The most an order holds. */
const val MOST = 10

/** An order. */
@Serializable
data class Order(val lines: List<Int>) : Priced {
    /** A new one. */
    constructor() : this(emptyList())

    override fun price(): Int = total() + helper(1)

    fun total(): Int {
        fun local() = 0
        return lines.sum() + local()
    }

    companion object {
        fun of(vararg lines: Int) = Order(lines.toList())
    }

    class Line(val count: Int)
}

interface Priced {
    fun price(): Int
}

enum class State { OPEN, SHUT }

object Registry {
    fun register(o: Order) {}
}

typealias Lines = List<Int>

fun Order.cheap(): Boolean = total() < 10

// A plain comment, then a blank line: not the function's.

/** Helps. */
fun helper(n: Int): Int = max(n, 1)
