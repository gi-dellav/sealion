// same as accumulate
const fold_right = accumulate;
length(fold_right(list, null, list(1, 2, 3)));
// result: [1, [[2, [[3, [null, null]], null]], null]]

// expected: 2
