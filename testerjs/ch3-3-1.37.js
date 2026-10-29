// solution provided by GitHub user clean99

function contains_cycle(x) {
    let counted_pairs = null;
    function is_counted_pair(counted_pairs, x) {
        return is_null(counted_pairs)
               ? false
               : head(counted_pairs) === x
               ? true
               : is_counted_pair(tail(counted_pairs), x);
    }
    function detect_cycle(x) {
        if (is_null(x)) {
            return false;
        } else if (is_counted_pair(counted_pairs, x)) {
            return true;
        } else {
            counted_pairs = pair(x, counted_pairs);
            return detect_cycle(tail(x));
        }
    }
    return detect_cycle(x);
}

const three_list = list("a", "b", "c");
const cycle = list("g", "h", "i");
set_tail(tail(tail(cycle)), cycle);

// displays false
display(contains_cycle(three_list));

// displays true
display(contains_cycle(cycle));
