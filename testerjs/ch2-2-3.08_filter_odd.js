function is_odd(n) {
    return n % 2 === 1;
}
filter(is_odd, list(1, 2, 3, 4, 5));

// expected: [ 1, [ 3, [ 5, null ] ] ]
