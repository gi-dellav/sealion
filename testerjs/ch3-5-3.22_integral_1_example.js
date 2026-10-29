function numbers_starting_from(t, dt) {
    return pair(t,
                () => numbers_starting_from(t + dt, dt)
               );
}
const dt = 0.01;
const linear = numbers_starting_from(0, dt);
const linear_integral = integral(linear, 0, dt);
// computing integral from 0 to 3 of f(x) = x
// (the integral is g(x) = 0.5 x^2, and therefore
// the result is near 0.5 * 3^2 = 4.5)
stream_ref(linear_integral, math_round(3 / dt));
