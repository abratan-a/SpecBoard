#include <stdio.h>

/* The most an order holds. */
#define MOST 10
#define TWICE(x) ((x) * 2)

static const int least = 1;

/* An order. */
struct order {
    int lines[MOST];
};

typedef struct order order_t;

enum state { OPEN, SHUT };

union value {
    int i;
    float f;
};

int helper(int n);

// A plain comment, then a blank line: not the function's.

/* Adds the lines up. */
int total(const struct order *o) {
    int sum = 0;
    for (int i = 0; i < MOST; i++) {
        sum += o->lines[i];
    }
    printf("%d", helper(sum));
    return TWICE(sum);
}

static int *pointer(void) { return 0; }
