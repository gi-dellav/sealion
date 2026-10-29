function plus(x, y) {
    return x + y;
}
// same as accumulate
const fold_right = accumulate;
fold_right(plus, 0, list(1, 2, 3));

// expected: 6
