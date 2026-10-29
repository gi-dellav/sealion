function square(x) {
    return x * x;
}
function is_divisible(x, y) { return x % y === 0; }
function integers_starting_from(n) {
    return pair(n, () => integers_starting_from(n + 1));
}
function is_prime(n) {
    function iter(ps) {
        return square(head(ps)) > n
               ? true
               : is_divisible(n, head(ps))
               ? false
               : iter(stream_tail(ps));
    }
    return iter(primes);
}

const primes = pair(2,
                    () => stream_filter(is_prime, 
                                        integers_starting_from(3)));

stream_ref(primes, 50);

// expected: 233
