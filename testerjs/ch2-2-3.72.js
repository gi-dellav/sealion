function enumerate_interval(low, high) {
    return low > high
           ? null
           : pair(low,
                  enumerate_interval(low + 1, high));
}
function flatmap(f, seq) {
    return accumulate(append, null, map(f, seq));
}
function unique_triples(n) {
    return flatmap(i => flatmap(j => map(k => list(i, j, k),
                                         enumerate_interval(1, j - 1)),
                                enumerate_interval(1, i - 1)),
                   enumerate_interval(1, n));
}
function plus(x, y) {
    return x + y;
}
function triples_that_sum_to(s, n) {
    return filter(items => accumulate(plus, 0, items) === s,
                  unique_triples(n));
}

length(triples_that_sum_to(10, 6));

// expected: 3
