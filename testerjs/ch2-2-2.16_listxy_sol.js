const x = list(1, 2, 3);

const y = list(4, 5, 6);
head(tail(list(x, y)));
// result: [[1, [2, [3, null]]], [[4, [5, [6, null]]], null]]

// expected: [ 4, [ 5, [ 6, null ] ] ]
