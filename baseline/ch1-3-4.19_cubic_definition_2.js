function abs(x) {
    return x >= 0 ? x : - x;
}
const tolerance = 0.00001;
function fixed_point(f, first_guess) {
    function close_enough(x, y) {
        return abs(x - y) < tolerance;
    }
    function try_with(guess) {
        const next = f(guess);
        return close_enough(guess, next)
               ? next
               : try_with(next);
    }
    return try_with(first_guess);
}
const dx = 0.00001;
function deriv(g) {
    return x => (g(x + dx) - g(x)) / dx;
}
function newton_transform(g) {
    return x => x - g(x) / deriv(g)(x);
}
function newtons_method(g, guess) {
    return fixed_point(newton_transform(g), guess);
}
function cube(x) {
    return x * x * x;
}
function square(x) {
    return x * x;
}
// cubic to be written by student; see EXERCISE 1.40
function cubic(a, b, c) {
    return x => cube(x) + a * square(x) + b * x + c;
}

newtons_method(cubic(1, -4, 0), 1);

// expected: 1.5615528128102987
