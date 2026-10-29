function scale_list(items, factor) {
    return map(x => x * factor, items);
}

tail(tail(scale_list(list(1, 2, 3, 4, 5), 10)));

// expected: [ 30, [ 40, [ 50, null ] ] ]
