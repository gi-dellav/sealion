function square(x) {
    return x * x;
}
function square_list(items) {
    return map(square, items);
}

tail(square_list(list(1, 2, 3, 4)));

// expected: [ 4, [ 9, [ 16, null ] ] ]
