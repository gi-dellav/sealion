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

adjoin_set(10, adjoin_set(15, adjoin_set(20, null)));

// expected: [ 10, [ 15, [ 20, null ] ] ]
