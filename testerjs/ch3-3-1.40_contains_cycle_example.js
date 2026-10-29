function last_pair(x) {
    return is_null(tail(x))
           ? x
           : last_pair(tail(x));
}
function make_cycle(x) {
    set_tail(last_pair(x), x);
    return x;
}
const c = make_cycle(list("c", "d", "e"));
const c1 = append(list("a", "b"), c);

contains_cycle(c1);
