function last_pair(x) {
    return is_null(tail(x))
           ? x
           : last_pair(tail(x));
}

last_pair(list(1, 2, 3, 4, 5));

// expected: [ 5, null ]
