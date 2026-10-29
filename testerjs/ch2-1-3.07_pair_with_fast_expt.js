function square(x) {
    return x * x;
}
function is_even(n) {
    return n % 2 === 0;
}
function fast_expt(b, n) {
    return n === 0
           ? 1
           : is_even(n)
           ? square(fast_expt(b, n / 2))
           : b * fast_expt(b, n - 1);
}
function pair(a, b) {
    return fast_expt(2, a) * fast_expt(3, b);
}
function head(p) {
    return p % 2 === 0
           ? head(p / 2) + 1
           : 0;
}
function tail(p) {
    return p % 3 === 0 
           ? tail(p / 3) + 1
           : 0;
}

tail(pair(3, 4));

// expected: 4
