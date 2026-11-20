function square(x) {
    return x * x;
}
function f_2(x, y) {
    return ( (a, b) => x * square(a) + y * b + a * b
           )(1 + x * y, 1 - y);
}

f_2(3, 4);

// expected: 456
