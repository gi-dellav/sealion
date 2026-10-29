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
function union_set(set1, set2) {
    if (is_null(set1)) {
        return set2;
    } else if (is_null(set2)) {
        return set1;
    } else {
      const x1 = head(set1);
      const x2 = head(set2);
      return x1 === x2
             ? pair(x1, union_set(tail(set1),
                                  tail(set2)))
             : x1 < x2 
             ? pair(x1, union_set(tail(set1), set2))
             : pair(x2, union_set(set1, tail(set2)));
    }
}

tail(union_set(
   adjoin_set(10, adjoin_set(20, adjoin_set(30, null))),
   adjoin_set(10, adjoin_set(15, adjoin_set(20, null)))));

// expected: [ 15, [ 20, [ 30, null ] ] ]
