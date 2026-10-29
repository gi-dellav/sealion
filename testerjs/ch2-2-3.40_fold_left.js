function fold_left(op, initial, sequence) {
    function iter(result, rest) {
        return is_null(rest)
               ? result
               : iter(op(result, head(rest)), 
                      tail(rest));
    }
    return iter(initial, sequence);
}

fold_left(list, null, list(1, 2, 3));

// expected: [ [ [ null, [ 1, null ] ], [ 2, null ] ], [ 3, null ] ]
