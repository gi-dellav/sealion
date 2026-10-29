function average(x, y) {
    return (x + y) / 2;
}
function sqrt_improve(guess, x) {
    return average(guess, x / guess);
}

sqrt_improve(1.2, 2);

// expected: 1.4333333333333333
