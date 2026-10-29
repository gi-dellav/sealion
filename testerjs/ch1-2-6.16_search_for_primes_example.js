function square(x) {
    return x * x;
}
function smallest_divisor(n) {
    return find_divisor(n, 2);
}
function find_divisor(n, test_divisor) {
    return square(test_divisor) > n
           ? n
           : divides(test_divisor, n)
           ? test_divisor
           : find_divisor(n, test_divisor + 1);
}
function divides(a, b) {
    return b % a === 0;
}
function is_prime(n) {
    return n === smallest_divisor(n);
}
function timed_prime_test(n) {
    display(n);
    return start_prime_test(n, get_time());
}

function start_prime_test(n, start_time) {
    return is_prime(n)
           ? report_prime(get_time() - start_time)
           : false;
}

function report_prime(elapsed_time) {
    display(" *** ");
    display(elapsed_time);
    return true;
}
function search_for_primes(start, times) {
    return times === 0
           ? true
           : start > 2 && start % 2 === 0
           ? search_for_primes(start + 1, times)
           // if we get true, it's a prime
           : timed_prime_test(start)
           ? search_for_primes(start + 2, times - 1)
           : search_for_primes(start + 2, times);
}
search_for_primes(10000, 3);
// search_for_primes(10000000, 3);
// search_for_primes(10000000000, 3);
