function fold_left(op, initial, sequence) {
    function iter(result, rest) {
        return is_null(rest)
               ? result
               : iter(op(result, head(rest)), 
                      tail(rest));
    }
    return iter(initial, sequence);
}
function reverse(sequence) {
    return fold_left((x, y) => pair(y, x), null, sequence);
}

head(reverse(list(1, 4, 5, 9, 16, 25)));

// expected: 25
