function inc(n) {
    return n + 1;
}
function square(x) {
    return x * x;
}
function compose(f, g) {
    return x => f(g(x));
}

compose(square, inc)(6); // returns 49

// expected: 49
