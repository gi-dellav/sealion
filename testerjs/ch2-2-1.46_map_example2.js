function map(fun, items) {
    return is_null(items)
           ? null
           : pair(fun(head(items)), 
                  map(fun, tail(items)));
}
tail(map(x => x * x, list(1, 2, 3, 4)));

// expected: [ 4, [ 9, [ 16, null ] ] ]
