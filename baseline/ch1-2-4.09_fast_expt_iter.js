function is_even(n) {
    return n % 2 === 0;
}
function fast_expt_iter(a, b, n){
    return n === 0
           ? a
           : is_even(n)
           ? fast_expt_iter(a, b * b, n / 2)
           : fast_expt_iter(a * b, b, n - 1);
}
function fast_expt(b, n){
    return fast_expt_iter(1, b, n);
}

fast_expt(2, 3);

// expected: 8
