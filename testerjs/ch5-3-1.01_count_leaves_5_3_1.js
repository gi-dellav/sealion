function count_leaves(tree) {
    return is_null(tree)
           ? 0
           : ! is_pair(tree)
           ? 1
           : count_leaves(head(tree)) +
             count_leaves(tail(tree));
}

count_leaves(pair(list(1, 2), list(3, 4)));

// expected: 4
