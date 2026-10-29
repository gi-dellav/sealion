function fold_left(op, initial, sequence) {
    function iter(result, rest) {
        return is_null(rest)
               ? result
               : iter(op(result, head(rest)), 
                      tail(rest));
    }
    return iter(initial, sequence);
}
function divide(x, y) {
    return x / y;
}
fold_left(divide, 1, list(1, 2, 3));
// result: 0.16666666666666666

// expected: 0.16666666666666666
