function square(x) {
    return x * x;
}
// tree_map to be written by student
function square_tree(tree) { return tree_map(square, tree); }
// square_tree to be written by student
function tree_map(f, tree) {
    return map(sub_tree => is_null(sub_tree)
                           ? null
                           : is_pair(sub_tree)
                           ? tree_map(f, sub_tree)
                           : f(sub_tree),
               tree);
}

tail(tail(square_tree(list(1,
                 list(2, list(3, 4), 5), 
                 list(6, 7)))));

// expected: [ [ 36, [ 49, null ] ], null ]
