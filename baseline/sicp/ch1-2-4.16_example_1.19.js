function is_even(n) {
    return n % 2 === 0;
}
function double(x) {
    return x + x;
}

function half(x) {
    return x / 2;
}

function fast_times_iter(total, a, b) {
    return b === 1
           ? total + a
           : a === 0 || b===0
           ? 0
           : is_even(b)
           ? fast_times_iter(total, double(a), half(b))
           : fast_times_iter(total + a, a, b - 1);
}

function times(a, b) {
  return fast_times_iter(0, a, b);
}
times(3, 4);

// expected: 12
