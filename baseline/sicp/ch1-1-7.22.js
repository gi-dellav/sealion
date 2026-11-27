function abs(x) {
    return x >= 0 ? x : - x;
}
function cube(x) {
    return x * x * x;
}
function is_good_enough(guess, x) {
    return abs(cube(guess) - x) < 0.001;
}
function div3(x, y) {
     return (x + y) / 3;
}
function improve(guess, x) {
    return div3(x / (guess * guess), 2 * guess);
}
function cube_root(guess, x) {
    return is_good_enough(guess, x)
           ? guess
           : cube_root(improve(guess, x), x);
}

cube_root(3, 27);
