function append(x, y) {
    return is_null(x)
           ? y
           : pair(head(x), append(tail(x), y));
}

list_ref(append(list(1, 3), list(5, 7, 9)), 4);

// expected: 9
