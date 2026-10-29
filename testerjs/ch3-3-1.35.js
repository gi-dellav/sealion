// solution provided by GitHub user clean99
	  
function count_pairs(x) {
    let counted_pairs = null;
    function is_counted_pair(current_counted_pairs, x) {
        return is_null(current_counted_pairs)
               ? false
               : head(current_counted_pairs) === x
               ? true
               : is_counted_pair(tail(current_counted_pairs), x);
    }
    function count(x) {
        if(! is_pair(x) || is_counted_pair(counted_pairs, x)) {
            return 0;
        } else {
            counted_pairs = pair(x, counted_pairs);
            return count(head(x)) +
                   count(tail(x)) +
                   1;
        }
    }
    return count(x);
}

const three_list = list("a", "b", "c");
const one = pair("d", "e");
const two = pair(one, one);
const four_list = pair(two, "f");
const seven_list = pair(two, two);
const cycle = list("g", "h", "i");
set_tail(tail(tail(cycle)), cycle);

// return 3; return 3; return 3;
display(count_pairs(three_list));
display(count_pairs(four_list));
display(count_pairs(seven_list));

// return 3
display(count_pairs(cycle));
