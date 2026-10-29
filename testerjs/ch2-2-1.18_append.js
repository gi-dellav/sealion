const squares = list(1, 4, 9, 16, 25);
const odds = list(1, 3, 5, 7);
function append(list1, list2) {
    return is_null(list1)
           ? list2
           : pair(head(list1), append(tail(list1), list2));
}

length(append(squares, odds));

// expected: 9
