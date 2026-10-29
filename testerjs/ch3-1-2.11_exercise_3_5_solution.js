function square(x) {
    return x * x;
}
// A very simple rand_update function computes a number
// from 0 (inclusive) to 200560490131 (a large prime)
// from a value x by multiplying it with a constant a,
// adding a constant b, and then taking the remainder
// of dividing it by the large prime. We used it here 
// for illustration only, and do not claim any 
// statistical properties.
const m = 200560490131; 
const a = 1103515245;
const b = 12345;

function rand_update(x) {
    return (a * x + b) % m;
}
const random_init = 123456789;
function make_rand() {
   let x = random_init;
   return () => {
              x = rand_update(x);
              return x;
          };
}
const rand = make_rand();
function gcd(a, b) {
    return b === 0 ? a : gcd(b, a % b);
}
function estimate_pi(trials) {
    return math_sqrt(6 / monte_carlo(trials, dirichlet_test));
}
function dirichlet_test() {
    return gcd(rand(), rand()) === 1;
}
function monte_carlo(trials, experiment) {
    function iter(trials_remaining, trials_passed) {
        return trials_remaining === 0
               ? trials_passed / trials
               : experiment()
               ? iter(trials_remaining - 1, trials_passed + 1)
               : iter(trials_remaining - 1, trials_passed);
    }
    return iter(trials, 0);
}
function random_in_range(low, high) {
    const range = high - low;
    return low + math_random() * range;
}
function estimate_integral(pred, x1, x2, y1, y2, trials) {
    const area_rect = (x2 - x1) * (y2 - y1);
    return monte_carlo(trials,
                       () => pred(random_in_range(x1, x2),
                                  random_in_range(y1, y2))) * area_rect;
}

function inside_unit_circle(x, y) {
    return square(x) + square(y) <= 1;
}
estimate_integral(inside_unit_circle, -1, 1, -1, 1, 50000);
