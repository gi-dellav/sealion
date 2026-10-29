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
function accumulate_r(combiner, null_value, term, a, next, b) {
    return a > b
           ? null_value
           : combiner(term(a), 
                      accumulate_r(combiner, 
                                   null_value, 
                                   term, next(a), next, b));
}
function sum_r(term, a, next, b) {
    function plus(x, y) {
        return x + y;
    }
    return accumulate_r(plus, 0, term, a, next, b);
}
function product_r(term, a, next, b) {
    function times(x, y) {
        return x * y;
    }
    return accumulate_r(times, 1, term, a, next, b);
}

//iterative process
function accumulate_i(combiner, null_value, term, a, next, b) {
    function iter(a, result) {
        return a > b
               ? result
               : iter(next(a), combiner(term(a), result));
    }
    return iter(a, null_value);
}
function sum_i(term, a, next, b) {
    function plus(x, y) {
        return x + y;
    }
    return accumulate_i(plus, 0, term, a, next, b);
}
function product_i(term, a, next, b) {
    function times(x, y) {
        return x * y;
    }
    return accumulate_i(times, 1, term, a, next, b);
}

factorial(5);

// expected: 120
