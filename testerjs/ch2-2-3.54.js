// same as accumulate
const fold_right = accumulate;
function reverse(sequence) {
    return fold_right((x, y) => append(y, list(x)), 
                      null, sequence);
}

head(reverse(list(1, 4, 5, 9, 16, 25)));

// expected: 25
