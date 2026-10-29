const sample_tree =
    make_code_tree(make_leaf("A", 4),
                   make_code_tree(
                       make_leaf("B", 2),
                       make_code_tree(
                           make_leaf("D", 1),
                           make_leaf("C", 1))));
const sample_frequencies = list(list("A", 4), 
                                list("B", 2), 
                                list("C", 1), 
                                list("D", 1));

equal(sample_tree, 
      generate_huffman_tree(sample_frequencies));
