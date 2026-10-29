const x = list(list(1, 2), list(3, 4));
function reverse(items) {
    function reverse_iter(items, result) {
        return is_null(items)
               ? result
               : reverse_iter(tail(items),
	                      pair(head(items), result));
    }
    return reverse_iter(items, null);
}
reverse(x);
