#include <stdio.h>

struct bag {
    long long a;
    long long b;
    long long c;
    long long d;
};

static long long obj_props(long long n) {
    struct bag o = {0, 0, 0, 0};
    long long acc = 0;
    for (long long i = 0; i < n; i += 1) {
        o.a += 1;
        o.b += 2;
        o.c += 3;
        long long v;
        switch (i & 3) {
        case 0:
            o.a += 4;
            v = o.a;
            break;
        case 1:
            o.b += 4;
            v = o.b;
            break;
        case 2:
            o.c += 4;
            v = o.c;
            break;
        default:
            o.d += 4;
            v = o.d;
            break;
        }
        acc = (acc + v + o.c) % 65536;
    }
    return acc;
}

int main(void) {
    printf("%lld\n", obj_props(300000LL) % 256);
    return 0;
}
