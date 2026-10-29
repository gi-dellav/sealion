function plus(x, y) {
    return x + y;
}
function fold_left(op, initial, sequence) {
    function iter(result, rest) {
        return is_null(rest)
               ? result
               : iter(op(result, head(rest)), 
                      tail(rest));
    }
    return iter(initial, sequence);
}
fold_left(plus, 0, list(1, 2, 3));

// expected: 6
