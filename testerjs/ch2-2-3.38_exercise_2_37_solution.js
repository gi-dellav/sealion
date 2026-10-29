function accumulate_n(op, init, seqs) {
    return is_null(head(seqs))
           ? null
           : pair(accumulate(op, init, map(x => head(x), seqs)),
                  accumulate_n(op, init, map(x => tail(x), seqs)));
}
function plus(x, y) {
    return x + y;
}
function times(x, y) {
    return x * y;
}
function dot_product(v, w) {
    return accumulate(plus, 0, accumulate_n(times, 1, list(v, w)));
}
function matrix_times_vector(m, v) {
    return map(row => dot_product(row, v), m);
}
function transpose(mat) {
    return accumulate_n(pair, null, mat);
}
function matrix_times_matrix(n, m) {
    const cols = transpose(m);
    return map(x => map(y => dot_product(x, y), cols), n);
}

const v = list(10, 20, 30);
const m1 = list(list(1, 2, 3), list(3, 5, 1), list(1, 1, 1));
const m2 = list(list(1, 2, 3), list(4, 5, 6), list(7, 8, 9));

matrix_times_vector(m1, v);
// transpose(m1);
// matrix_times_matrix(m1, m2);

// expected: [ 140, [ 160, [ 60, null ] ] ]
