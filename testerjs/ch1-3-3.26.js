//iterative process
function cont_frac(n, d, k) {
    function fraction(i, current) {
        return i === 0
               ? current
               : fraction(i - 1, n(i) / (d(i) + current));
    }
    return fraction(k, 0);
}
function tan_cf(x, k) {
    return cont_frac(i => i === 1 ? x : - x * x,  
                     i => 2 * i - 1,
                     k);
}

tan_cf(math_PI, 14);
// math_tan(math_PI);

// expected: -2.8271597168564594e-16
