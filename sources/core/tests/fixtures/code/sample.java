package shop;

import java.util.List;

/** An order. */
@Entity
public class Order implements Priced {
    /** The most an order holds. */
    public static final int MOST = 10;

    private List<Integer> lines;

    /** A new one. */
    public Order() {
        this.lines = List.of();
    }

    public Order(List<Integer> lines) {
        this.lines = lines;
    }

    @Override
    public int price() {
        return total() + Helper.help(1);
    }

    int total() {
        Runnable r = () -> {};
        r.run();
        return lines.stream().mapToInt(Integer::intValue).sum();
    }

    static class Line {
        int count;
    }
}

interface Priced {
    int price();
}

enum State {
    OPEN,
    SHUT;

    State flipped() {
        return this == OPEN ? SHUT : OPEN;
    }
}

record Point(int x, int y) {}

@interface Marked {}
