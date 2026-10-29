const one_through_four = list(1, 2, 3, 4);
tail(tail(pair(10, one_through_four)));

// expected: [ 2, [ 3, [ 4, null ] ] ]
