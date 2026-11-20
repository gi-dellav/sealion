function compose(f, g) {
    return x => f(g(x));
}
function square(x) {
    return x * x;
}
function repeated(f, n) {
    return n === 0
           ? x => x
           : compose(f, repeated(f, n - 1));
}

repeated(square, 2)(5); // 625

// expected: 625
