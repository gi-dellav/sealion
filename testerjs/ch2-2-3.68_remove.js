function remove(item, sequence) {
    return filter(x => ! (x === item),
                  sequence);
}

length(remove(3, list(1, 2, 3, 4, 5)));

// expected: 4
