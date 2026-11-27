function factorial(n) {
    function term(i) {
        return i;
    }
    function next(i) {
        return i + 1;
    }
    return product_r(term, 1, next, n);
}
