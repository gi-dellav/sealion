import { head, is_null, list, list_ref, pair, tail } from './sicp.js';
function square(x) {
    return x * x;
}
function map(fun, items) {
    return is_null(items)
           ? null
           : pair(fun(head(items)), 
                  map(fun, tail(items)));
}
list_ref(map(square, list(1, 2, 3, 4, 5)), 4);

// expected: 25
