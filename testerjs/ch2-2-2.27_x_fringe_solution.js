const x = list(list(1, 2), list(3, 4));
// fringe to be written by student
function fringe(x) {
    return is_null(x)
           ? null
           : is_pair(x)
           ? append(fringe(head(x)), fringe(tail(x)))
           : list(x);
}

length(fringe(list(x, x)));

// expected: 8
