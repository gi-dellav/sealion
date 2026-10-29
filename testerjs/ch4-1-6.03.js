// solution provided by GitHub user LucasGdosR

function f(x) {
    return ((is_even, is_odd) => is_even(is_even, is_odd, x))
           ((is_ev, is_od, n) => n === 0 ? true : is_od(is_ev, is_od, n - 1),
           (is_ev, is_od, n) => n === 0 ? false : is_ev(is_ev, is_od, n - 1));
}
