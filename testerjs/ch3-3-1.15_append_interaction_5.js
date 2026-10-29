function append(x, y) {
    return is_null(x)
           ? y
           : pair(head(x), append(tail(x), y));
}
function last_pair(x) {
    return is_null(tail(x))
           ? x
           : last_pair(tail(x));
}
function append_mutator(x, y) {
    set_tail(last_pair(x), y);
    return x;
}
const x = list("a", "b");
const y = list("c", "d");
const z = append(x, y);
z;
tail(x);
