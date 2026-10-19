#include <vector>

/// The most an order holds.
constexpr int MOST = 10;

namespace shop {

/// An order.
class Order {
public:
    /// A new one.
    Order();
    int total() const;
    int total(int extra) const { return total() + extra; }

    struct Line {
        int count;
    };

private:
    std::vector<int> lines;
};

template <typename T>
T twice(T x) {
    return x * 2;
}

Order::Order() {}

int Order::total() const {
    int sum = 0;
    for (auto n : lines) sum += n;
    return twice(sum);
}

enum class State { Open, Shut };

using Lines = std::vector<int>;

}  // namespace shop

// A plain comment, then a blank line: not the function's.

/// Helps.
int helper(int n) {
    shop::Order o;
    return o.total() + n;
}
