function is_even(n) {
    return n % 2 === 0;
}
function square(x) {
    return x * x;
}
function carmichael(n) {
    function expmod(base, exp, m) {
        return exp === 0
               ? 1
               : is_even(exp)
               ? square(expmod(base, exp / 2, m)) % m
               : (base * expmod(base, exp - 1, m)) % m;
    }
    function fermat_test(n, a) {
        return expmod(a, n, n) === a;
    }
    function iter(n, i) {
        return i === n
               ? true
               : fermat_test(n, i)
               ? iter(n, i + 1)
               : false;
    }
    return iter(n, 2);
}

carmichael(561);
// carmichael(1105);
// carmichael(1729);
// carmichael(2465);
// carmichael(2821);
// carmichael(6601);
