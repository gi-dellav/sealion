function square(x) {
    return x * x;
}
function is_even(n) {
    return n % 2 === 0;
}
function expmod(base, exp, m) {
    return exp === 0
           ? 1
           : is_even(exp)
           ? square(expmod(base, exp / 2, m)) % m
           : (base * expmod(base, exp - 1, m)) % m;
}
function random(n) {
    return math_floor(math_random() * n);
}
function fermat_test(n) {
    function try_it(a) {
        return expmod(a, n, n) === a;
    }
    return try_it(1 + math_floor(math_random() * (n - 1)));
}
function fast_is_prime(n, times) {
    return times === 0
           ? true
           : fermat_test(n)
           ? fast_is_prime(n, times - 1)
           : false;
}
function timed_prime_test(n) {
    display(n);
    return start_prime_test(n, get_time());
}
function start_prime_test(n, start_time) {
    return fast_is_prime(n, math_floor(math_log(n)))
           ? report_prime(get_time() - start_time)
           : true;
}
function report_prime(elapsed_time) {
    display(" *** ");
    display(elapsed_time);
}

timed_prime_test(10007);
// timed_prime_test(10009);
// timed_prime_test(10037);
// timed_prime_test(100003, 3);
// timed_prime_test(100019, 3);
// timed_prime_test(100043, 3);
// timed_prime_test(1000003, 3);
// timed_prime_test(1000033, 3);
// timed_prime_test(1000037, 3);
