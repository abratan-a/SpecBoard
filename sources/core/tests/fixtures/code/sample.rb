require "set"

# The shop.
module Shop
  # The most an order holds.
  MOST = 10

  # An order.
  class Order < Base
    attr_reader :lines

    # A new one.
    def initialize
      @lines = []
    end

    def total
      lines.sum + helper(1)
    end

    def self.of(lines)
      new.tap { |o| o.lines.concat(lines) }
    end

    def ==(other)
      total == other.total
    end
  end
end

class Shop::Order
  def cheap?
    total < 10
  end
end

# A plain comment, then a blank line: not the function's.

# Helps.
def helper(n)
  n + 1
end
