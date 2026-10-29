function make_rat(n, d) { return pair(n, d); }

function numer(x) { return head(x); }

function denom(x) { return tail(x); }

numer(make_rat(2, 3));

// expected: 2
