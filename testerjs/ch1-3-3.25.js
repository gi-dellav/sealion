//iterative process
function cont_frac(n, d, k) {
    function fraction(i, current) {
        return i === 0
               ? current
               : fraction(i - 1, n(i) / (d(i) + current));
    }
    return fraction(k, 0);
}
2 + cont_frac(i => 1,  
              i => (i + 1) % 3 < 1 ? 2 * (i + 1) / 3 : 1,
              20);

// expected: 2.718281828459045
