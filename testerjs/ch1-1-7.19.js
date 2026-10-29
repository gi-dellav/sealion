function abs(x) {
    return x >= 0 ? x : - x;
}
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
function square(x) {
    return x * x;
}
const relative_tolerance = 0.0001;
function is_good_enough(guess, x) {
    return abs(square(guess) - x) < guess * relative_tolerance;
}

display(sqrt(0.0001));
display(sqrt(4000000000000));
