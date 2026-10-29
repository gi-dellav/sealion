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
                    () => stream_filter(
                              is_prime, 
                              integers_starting_from(3))
                   );
function interleave(s1, s2) {
    return is_null(s1)
           ? s2
           : pair(head(s1), 
                  () => interleave(s2, stream_tail(s1)));
}
function pairs(s, t) {
    return pair(list(head(s), head(t)), 
                () => interleave(stream_map(x => list(head(s), x),
                                            stream_tail(t)),
                                 pairs(stream_tail(s), 
                                       stream_tail(t))));
}
const integers = integers_starting_from(1);
const max_display = 9;
function display_stream(s) {
    function display_stream_iter(st, n) {
        if (is_null(st)) {
        } else if (n === 0) {
            display('', "...");
        } else {
            display(head(st));
            display_stream_iter(stream_tail(st), n - 1);
        }
    }
    display_stream_iter(s, max_display);
}
const int_pairs = pairs(integers, integers);
stream_ref(stream_filter(p => is_prime(head(p) + head(tail(p))),
              int_pairs), 8);

// expected: [ 1, [ 12, null ] ]
