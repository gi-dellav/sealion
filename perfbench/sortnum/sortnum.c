#include <stdio.h>
#include <stdlib.h>

static int cmp(const void *pa, const void *pb) {
    long long a = *(const long long *)pa;
    long long b = *(const long long *)pb;
    if (a < b) {
        return -1;
    }
    if (a > b) {
        return 1;
    }
    return 0;
}

int main(void) {
    const long long n = 200000;
    long long *a = malloc(sizeof(long long) * (size_t)n);
    if (a == NULL) {
        return 1;
    }
    unsigned int x = 2463534242u;
    for (long long i = 0; i < n; i += 1) {
        x = x ^ (x << 13);
        x = x ^ (x >> 17);
        x = x ^ (x << 5);
        a[i] = (long long)((x >> 8) % 1000);
    }
    qsort(a, (size_t)n, sizeof(long long), cmp);
    long long acc = 0;
    for (long long i = 0; i < n; i += 1) {
        acc = (acc + (a[i] % 256) * ((i % 7) + 1)) % 65536;
    }
    printf("%lld\n", acc % 256);
    free(a);
    return 0;
}
