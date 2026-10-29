function inside_unit_circle(x, y) {
    return square(x) + square(y) <= 1;
}
estimate_integral(inside_unit_circle, -1, 1, -1, 1, 50000);
