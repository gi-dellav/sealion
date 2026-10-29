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
    return math_sqrt(6 / random_gcd_test(trials, random_init));
}
function random_gcd_test(trials, initial_x) {
    function iter(trials_remaining, trials_passed, x) {
        const x1 = rand_update(x);
        const x2 = rand_update(x1);
        return trials_remaining === 0
               ? trials_passed / trials
               : gcd(x1, x2) === 1
               ? iter(trials_remaining - 1, trials_passed + 1, x2)
               : iter(trials_remaining - 1, trials_passed, x2);
    }
    return iter(trials, 0, initial_x);
}

estimate_pi(10000);

// expected: 3.1408877612819492
