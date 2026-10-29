function is_divisible(x, y) { return x % y === 0; }
function integers_starting_from(n) {
    return pair(n, () => integers_starting_from(n + 1));
}
function sieve(stream) {
    return pair(head(stream),
                () => sieve(stream_filter(
                                x => ! is_divisible(x, head(stream)),
                                stream_tail(stream))));
}
const primes = sieve(integers_starting_from(2));
stream_ref(primes, 50);

// expected: 233
