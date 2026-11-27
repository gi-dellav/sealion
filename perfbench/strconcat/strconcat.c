#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static long long str_concat(long long rounds, long long pieces) {
    size_t cap = (size_t)(pieces * 8) + 1;
    char *s = malloc(cap);
    if (s == NULL) {
        return -1;
    }
    long long acc = 0;
    for (long long r = 0; r < rounds; r += 1) {
        size_t len = 0;
        for (long long i = 0; i < pieces; i += 1) {
            memcpy(s + len, "abcdefgh", 8);
            len += 8;
        }
        s[len] = '\0';
        acc = (acc + (long long)len + (long long)(unsigned char)s[r]) % 65536;
    }
    free(s);
    return acc;
}

int main(void) {
    printf("%lld\n", str_concat(200LL, 2000LL) % 256);
    return 0;
}
