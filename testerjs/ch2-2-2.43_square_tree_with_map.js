function square(x) {
    return x * x;
}
// square_tree to be written by student
function square_tree(tree) {
    return map(sub_tree => ! is_pair(sub_tree)
                           ? square(sub_tree)
                           : square_tree(sub_tree),
               tree);
}

tail(tail(square_tree(list(1,
                 list(2, list(3, 4), 5), 
                 list(6, 7)))));

// expected: [ [ 36, [ 49, null ] ], null ]
