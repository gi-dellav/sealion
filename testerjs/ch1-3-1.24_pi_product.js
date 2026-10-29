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
