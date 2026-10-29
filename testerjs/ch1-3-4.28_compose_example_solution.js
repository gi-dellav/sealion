function inc(n) {
    return n + 1;
}
function square(x) {
    return x * x;
}
compose(square, inc)(6); // returns 49
