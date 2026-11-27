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

rand();
rand();
rand();

// expected: 40083849805
