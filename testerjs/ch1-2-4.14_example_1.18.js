function is_even(n) {
    return n % 2 === 0;
}
function double(x) {
    return x + x;
}

function halve(x) {
    return x / 2;
}

function fast_times(a, b) { 
    return b === 1
           ? a
           : a === 0 || b === 0
           ? 0
           : is_even(b)
           ? double(fast_times(a, halve(b)))
           : a + fast_times(a, b - 1);
}
fast_times(3, 4);

// expected: 12
