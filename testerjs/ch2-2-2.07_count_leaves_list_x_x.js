const x = pair(list(1, 2), list(3, 4));
function count_leaves(x) {
    return is_null(x)
           ? 0
           : ! is_pair(x)
           ? 1
           : count_leaves(head(x)) + count_leaves(tail(x));
}
count_leaves(list(x, x));

// expected: 8
