function make_rat(n, d) { return pair(n, d); }

function numer(x) { return head(x); }

function denom(x) { return tail(x); }
function print_rat(x) {
    return display(stringify(numer(x)) + " / " + stringify(denom(x)));
}
const one_half = make_rat(1, 2);

one_half;

// expected: [ 1, 2 ]
