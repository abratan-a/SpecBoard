"""A module's own doc, not a symbol's."""

import os

# The most an order holds.
MOST = 10


# An order.
@dataclass
class Order:
    """An order."""

    lines: list

    def __init__(self):
        def local():
            pass

        local()

    @property
    def total(self):
        return sum(self.lines) + helper(1)

    class Line:
        count: int


# A plain comment, then a blank line: not the function's.

def helper(n):
    f = lambda x: x + 1
    return f(n)


async def fetch():
    await os.sleep(1)
