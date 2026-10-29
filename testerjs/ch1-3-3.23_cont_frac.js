// cont_frac to be written by student; see exercise 1.37
//iterative process
function cont_frac(n, d, k) {
    function fraction(i, current) {
        return i === 0
               ? current
               : fraction(i - 1, n(i) / (d(i) + current));
    }
    return fraction(k, 0);
}

cont_frac(i => 1, i => 1, 20);

// expected: 0.6180339850173578
