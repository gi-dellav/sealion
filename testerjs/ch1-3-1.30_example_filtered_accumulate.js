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
function inc(n) {
    return n + 1;
}
function gcd(a, b) {
    return b === 0 ? a : gcd(b, a % b);
}
function prime_squares_sum(a, b) {
    function plus(x, y) {
        return x + y;
    }
    return filtered_accumulate(plus, 0,
                               square, a, inc, b, 
                               is_prime);
}
function relative_prime_product(n) {    
    function times(x, y) {
        return x * y;
    }
    function identity(x) {
        return x;
    }
    function test(i) {
        return gcd(i, n) === 1;
    }
    return filtered_accumulate(times, 1,
                               identity, 1, inc, n,
                               test);
}

prime_squares_sum(4, 10); // 74
// relative_prime_product(8); // 105
