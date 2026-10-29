function is_element_of_set(x, set) {
    return is_null(set) 
           ? false
           : equal(x, head(set))
           ? true
           : is_element_of_set(x, tail(set));
}
function adjoin_set(x, set) {
    return is_element_of_set(x, set)
           ? set
           : pair(x, set);
}
function union_set(set1, set2) {
    return is_null(set1)
           ? set2
           : adjoin_set(head(set1), 
                        union_set(tail(set1), set2));
}

tail(tail(tail(union_set(
   adjoin_set(10, adjoin_set(20, adjoin_set(30, null))),
   adjoin_set(10, adjoin_set(15, adjoin_set(20, null)))))));

// expected: [ 20, null ]
