#include <stdint.h>
#include <stdio.h>

static long long xorshift(long long n) {
    uint32_t x = 2463534242u;
    long long acc = 0;
    for (long long i = 0; i < n; i += 1) {
        x = x ^ (x << 13);
        x = x ^ (x >> 17);
        x = x ^ (x << 5);
        acc = (acc + (long long)x) % 65536;
    }
    return acc;
}

int main(void) {
    printf("%lld\n", xorshift(1000000LL) % 256);
    return 0;
}
