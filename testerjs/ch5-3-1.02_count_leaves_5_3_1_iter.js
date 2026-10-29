function count_leaves(tree) {
    function count_iter(tree, n) {
        return is_null(tree)
               ? n
               : ! is_pair(tree) 
               ? n + 1
               : count_iter(tail(tree),
                            count_iter(head(tree), n));
    }
    return count_iter(tree, 0);
}

count_leaves(pair(list(1, 2), list(3, 4)));

// expected: 4
