function map(f, sequence) {
    return accumulate((x, y) => pair(f(x), y),
                      null, 
                      sequence);
}
function append(seq1, seq2) {
    return accumulate(pair, seq2, seq1);
}
function length(sequence) {
    return accumulate((x, y) => y + 1,
                      0, 
                      sequence);
}

tail(map(math_sqrt, list(1, 2, 3, 4)));

// expected: [ 1.4142135623730951, [ 1.7320508075688772, [ 2, null ] ] ]
