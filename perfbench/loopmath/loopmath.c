#include <math.h>
#include <stdio.h>

double loop_math(long long n) {
    double acc = 0.0;
    for (long long i = 1; i <= n; i += 1) {
        double x = (double)i;
        acc = acc + sqrt(x) + (x * 0.5) / (x + 1.0);
    }
    return acc;
}

int main(void) {
    long long result = (long long)loop_math(10000000LL);
    printf("%lld\n", result % 256);
    return 0;
}
