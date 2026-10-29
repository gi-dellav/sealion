function integers_starting_from(n) {
    return pair(n, () => integers_starting_from(n + 1));
}
const integers = integers_starting_from(1);
function is_divisible(x, y) { return x % y === 0; }
const no_sevens = stream_filter(x => ! is_divisible(x, 7),
                                integers);
stream_ref(no_sevens, 100);

// expected: 117
