function is_even(n) {
    return n % 2 === 0;
}
function square(x) {
    return x * x;
}
function random(n) {
    return math_floor(math_random() * n);
}
function miller_rabin_test(n) {
    function expmod(base, exp, m) {
        return exp === 0
               ? 1
               : is_even(exp)
               ? square(trivial_test(expmod(base,
                                            exp / 2,
                                            m), 
                                     m))
                 % m
               : (base * expmod(base, exp - 1, m)) 
                 % m;
    }
    function trivial_test(r, m) {
        return r === 1 || r === m - 1
               ? r
               : square(r) % m === 1
               ? 0
               : r;
    }
    function try_it(a) {
        return expmod(a, n - 1, n) === 1;
    }
    return try_it(1 + random(n - 1));
}
function do_miller_rabin_test(n, times) {
    return times === 0
           ? true
           : miller_rabin_test(n)
           ? do_miller_rabin_test(n, times - 1)
           : false;
}

do_miller_rabin_test(104743, 1000);
