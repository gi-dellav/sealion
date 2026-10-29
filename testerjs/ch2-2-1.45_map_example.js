function abs(x) {
    return x >= 0 ? x : - x;
}
function map(fun, items) {
    return is_null(items)
           ? null
           : pair(fun(head(items)), 
                  map(fun, tail(items)));
}
tail(map(abs, list(-10, 2.5, -11.6, 17)));

// expected: [ 2.5, [ 11.6, [ 17, null ] ] ]
