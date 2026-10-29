function count_leaves(t) {
    return accumulate((leaves, total) => leaves + total,
                      0, 
		      map(sub_tree => is_pair(sub_tree)
		                      ? count_leaves(sub_tree)
                                      : 1,
			  t));
}

count_leaves(pair(list(1, 2), list(3, 4)));

// expected: 4
