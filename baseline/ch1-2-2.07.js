// iterative function
function f_iterative(n) {
    return n < 3
           ? n
           : f_iterative_impl(2, 1, 0, n - 2);
}
function f_iterative_impl(a, b, c, count) {
    return count === 0
           ? a
           : f_iterative_impl(a + 2 * b + 3 * c, a, b, count - 1);
}

f_iterative(5);

// expected: 25
