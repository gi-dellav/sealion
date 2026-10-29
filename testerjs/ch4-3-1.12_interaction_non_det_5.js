// chapter=3 variant=non-det 
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
function prime_sum_pair(list1, list2) {    
    const a = an_element_of(list1);
    const b = an_element_of(list2);
    require(is_prime(a + b));
    return list(a, b);
}
prime_sum_pair(list(19, 27, 30), list(11, 36, 58));
// Press "Run" for the first solution. Type
// retry
// in the REPL on the right, for more solutions
