function reverse(items) {
    function reverse_iter(items, result) {
        return is_null(items)
               ? result
               : reverse_iter(tail(items),
	                      pair(head(items), result));
    }
    return reverse_iter(items, null);
}

head(reverse(list(1, 4, 5, 9, 16, 25)));

// expected: 25
