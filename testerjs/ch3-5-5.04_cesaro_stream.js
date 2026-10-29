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
const random_numbers =
    pair(random_init,
         () => stream_map(rand_update, random_numbers));
function gcd(a, b) {
    return b === 0 ? a : gcd(b, a % b);
}
function map_successive_pairs(f, s) {
    return pair(f(head(s), head(stream_tail(s))),
                () => map_successive_pairs(
                          f,
                          stream_tail(stream_tail(s))));
}
const dirichlet_stream =
    map_successive_pairs((r1, r2) => gcd(r1, r2) === 1,
                         random_numbers);

stream_ref(dirichlet_stream, 42);

// expected: true
