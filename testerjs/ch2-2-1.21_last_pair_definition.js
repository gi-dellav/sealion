// last_pair to be given by student
function last_pair(items) {
    return is_null(tail(items))
           ? items
           : last_pair(tail(items));
}

last_pair(list(23, 72, 149, 34));

// expected: [ 34, null ]
