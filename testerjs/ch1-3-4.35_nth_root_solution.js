function abs(x) {
    return x >= 0 ? x : - x;
}
const tolerance = 0.00001;
function fixed_point(f, first_guess) {
    function close_enough(x, y) {
        return abs(x - y) < tolerance;
    }
    function try_with(guess) {
        const next = f(guess);
        return close_enough(guess, next)
               ? next
               : try_with(next);
    }
    return try_with(first_guess);
}
function average(x, y) {
    return (x + y) / 2;
}
function average_damp(f) {
    return x => average(x, f(x));
}
function compose(f, g) {
    return x => f(g(x));
}
function repeated(f, n) {
    return n === 0
           ? x => x
           : compose(f, repeated(f, n - 1));
}
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
function nth_root(n, x) {
    return fixed_point(repeated(average_damp, 
                                math_floor(math_log2(n)))
                       (y => x / fast_expt(y, n - 1)),
                       1);
}

nth_root(5, 32);

// expected: 2.000001512995761
