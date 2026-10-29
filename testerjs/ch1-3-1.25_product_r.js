function factorial(n) {
    function term(i) {
        return i;
    }
    function next(i) {
        return i + 1;
    }
    return product_r(term, 1, next, n);
}
//pi product
function pi(n) {
    function is_odd(i) {
        return i % 2 === 1;
    }
    function term(i) {
        return is_odd(i)
               ? (i + 1) / (i + 2)
               : (i + 2) / (i + 1);
    }
    function next(i) {
        return i + 1;
    }
    return 4 * (product_i(term, 1.0, next, n));
}
//recursive process
function product_r(term, a, next, b) {
    return a > b
           ? 1
           : term(a) * product_r(term, next(a), next, b);
}

//iterative process
function product_i(term, a, next, b) {
    function iter(a, result) {
        return a > b
               ? result
               : iter(next(a), term(a) * result);
    }
    return iter(a, 1);
}

factorial(5);
// pi(17);

// expected: 120
