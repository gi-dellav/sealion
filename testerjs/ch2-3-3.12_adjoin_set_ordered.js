function adjoin_set(x, set) {
    return is_null(set)
           ? list(x)
           : x === head(set) 
           ? set
           : x < head(set)
           ? pair(x, set)
           : pair(head(set), 
                  adjoin_set(x, tail(set)));
}

adjoin_set(10, adjoin_set(15, adjoin_set(20, null)));

// expected: [ 10, [ 15, [ 20, null ] ] ]
