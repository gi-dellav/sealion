function integers_starting_from(n) {
    return pair(n, () => integers_starting_from(n + 1));
}

const from_20 = integers_starting_from(20);
stream_ref(from_20, 50);

// expected: 70
