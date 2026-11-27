function square(x) {
    return x * x;
}
function average(x, y) {
    return (x + y) / 2;
}
function improve(guess, x) {
    return average(guess, x / guess);
}
function abs(x) {
    return x >= 0 ? x : - x;
}
function is_good_enough(guess, x) {
    return abs(square(guess) - x) < 0.001;
}
function iterative_improve(is_good_enough, improve) {
    function iterate(guess) {
        return is_good_enough(guess)
               ? guess
               : iterate(improve(guess));
    }
    return iterate;
}
function sqrt(x) {
    return iterative_improve(y => is_good_enough(y, x), 
                             y => improve(y, x))(1);
}

sqrt(49);
