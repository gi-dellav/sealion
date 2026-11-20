function cube(x) {
    return x * x * x;
}
function sum(term, a, next, b) {
    return a > b
           ? 0
           : term(a) + sum(term, next(a), next, b);
}
function inc(k) {
    return k + 1;
}
function simpsons_rule_integral(f, a, b, n) {
    function helper(h) {
        function y(k) { 
            return f((k * h) + a);
        }
	function term(k) {
            return k === 0 || k === n
                   ? y(k)
                   : k % 2 === 0
                     ? 2 * y(k)
                     : 4 * y(k);
        }
        return sum(term, 0, inc, n) * (h / 3);
    }
    return helper((b - a) / n);
}

simpsons_rule_integral(cube, 0, 1, 100);

// expected: 0.24999999999999992
