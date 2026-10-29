function abs(x) {
    return x >= 0 ? x : - x;
}
function fixed_point_with_average_dampening(f, first_guess) {
    function close_enough(x, y) {
        return abs(x - y) < tolerance;
    }
    function try_with(guess) {
        display(guess);
        const next = (guess + f(guess)) / 2;
        return close_enough(guess, next)
               ? next
               : try_with(next);
    }
    return try_with(first_guess);
}

fixed_point_with_average_dampening(x => math_log(1000) / math_log(x), 2.0);
