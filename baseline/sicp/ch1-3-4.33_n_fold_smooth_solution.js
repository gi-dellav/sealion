function cube(x) {
    return x * x * x;
}
function compose(f, g) {
    return x => f(g(x));
}
function repeated(f, n) {
    return n === 0
           ? x => x
           : compose(f, repeated(f, n - 1));
}
const dx = 0.00001;
function smooth(f) {
    return x => (f(x - dx) + f(x) + f(x + dx)) / 3;
}
function n_fold_smooth(f, n) {
    return repeated(smooth, n)(f);
}

n_fold_smooth(cube, 5)(4);

// expected: 64.00000000399997
