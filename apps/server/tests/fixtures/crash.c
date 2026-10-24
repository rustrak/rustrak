#include <stdlib.h>

static inline __attribute__((always_inline)) int divide(int a, int b) {
    if (b == 0) abort();
    return a / b;
}

__attribute__((noinline)) int compute(int x) {
    int y = x - 7;
    return divide(100, y);
}

int main(int argc, char **argv) {
    return compute(argc + 6);
}
