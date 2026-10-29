function cube(x) {
    return x * x * x;
}
function sum(term, a, next, b) {
    function iter(a, result) {
        return a > b
               ? result
               : iter(next(a), result + term(a));
    }
    return iter(a, 0);
}

function inc(n) {
    return n + 1;
}
function sum_cubes(a, b) {
    return sum(cube, a, inc, b);
}
sum_cubes(1, 10);

// expected: 3025
