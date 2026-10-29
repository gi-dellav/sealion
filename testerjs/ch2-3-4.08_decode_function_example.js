const my_leaf_1 = make_leaf("A", 8);
const my_leaf_2 = make_leaf("B", 3);

const my_tree = make_code_tree(my_leaf_1, my_leaf_2);

tail(decode(list(0, 1, 1, 0), my_tree));
