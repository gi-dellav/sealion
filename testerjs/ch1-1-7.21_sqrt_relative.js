function average(x, y) {
    return (x + y) / 2;
}
function improve(guess, x) {
    return average(guess, x / guess);
}
function sqrt_iter(guess, x) {
    return is_good_enough(guess, x)
           ? guess
           : sqrt_iter(improve(guess, x), x);
}

function sqrt(x) {
    return sqrt_iter(1, x);
}
