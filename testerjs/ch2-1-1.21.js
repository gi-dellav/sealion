function abs(x) {
    return x >= 0 ? x : - x;
}
function gcd(a, b) {
    return b === 0 ? a : gcd(b, a % b);
}
function sign(x) {
    return x < 0 
           ? -1
	   : x > 0
	     ? 1
	     : 0;
}
function make_rat(n, d) {
    const g = gcd(n, d);
    return pair(sign(n) * sign(d) * abs(n / g), 
                abs(d / g));
}

make_rat(3, -4);

// expected: [ -3, 4 ]
