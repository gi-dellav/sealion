function adjoin_set(x, set) {
    return is_element_of_set(x, set)
           ? set
           : pair(x, set);
}
function is_element_of_set(x, set) {
    return is_null(set) 
           ? false
           : equal(x, head(set))
           ? true
           : is_element_of_set(x, tail(set));
}

is_element_of_set(15,
    adjoin_set(10, adjoin_set(15, adjoin_set(20, null))));

// expected: true
