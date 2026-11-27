// cont_frac to be written by student; see exercise 1.37
//recursive process
function cont_frac(n, d, k) {
    function fraction(i) {
        return i > k
               ? 0
               : n(i) / (d(i) + fraction(i + 1));
    }
    return fraction(1);
}

cont_frac(i => 1, i => 1, 20);

// expected: 0.6180339850173578
