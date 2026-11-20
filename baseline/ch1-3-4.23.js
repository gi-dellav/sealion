function inc(n) {
    return n + 1;
}
function double(f) {
    return x => f(f(x));
}

double(double(double))(inc)(5); // returns 21

// expected: 21
