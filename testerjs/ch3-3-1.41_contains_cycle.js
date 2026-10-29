function last_pair(x) {
    return is_null(tail(x))
           ? x
           : last_pair(tail(x));
}
function make_cycle(x) {
    set_tail(last_pair(x), x);
    return x;
}
function contains_cycle(x) {
    function detect_cycle(fast, slow) {
        return is_null(fast) || is_null(tail(fast))
               ? false
               : fast === slow
               ? true
               : detect_cycle(tail(tail(fast)), tail(slow));
    }
    return detect_cycle(tail(x), x);
}

const c = make_cycle(list("c", "d", "e"));
const c1 = append(list("a", "b"), c);

contains_cycle(c1);
