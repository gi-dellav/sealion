function square(x) {
    return x * x;
}
function f_3(x, y) {
    const a = 1 + x * y;
    const b = 1 - y;
    return x * square(a) + y * b + a * b;
}

f_3(3, 4);

// expected: 456
