#include <stdio.h>
#include <stdlib.h>

static long long char_mix(long long passes, long long n) {
    char *src = malloc((size_t)n + 1);
    if (src == NULL) {
        return -1;
    }
    for (long long i = 0; i < n; i += 1) {
        src[i] = (char)(65 + (i % 26));
    }
    src[n] = '\0';
    long long acc = 0;
    for (long long p = 0; p < passes; p += 1) {
        for (long long i = 0; i < n; i += 1) {
            acc = (acc + (long long)(unsigned char)src[i] * ((i % 7) + 1)) % 65536;
        }
    }
    free(src);
    return acc;
}

int main(void) {
    printf("%lld\n", char_mix(1000LL, 1024LL) % 256);
    return 0;
}
