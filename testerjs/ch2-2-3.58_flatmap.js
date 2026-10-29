function flatmap(f, seq) {
    return accumulate(append, null, map(f, seq));
}

length(flatmap(x => list(x, x), list(1, 2, 3, 4)));

// expected: 8
