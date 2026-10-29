const x = list(list(1, 2), list(3, 4));
// deep_reverse to be written by student
function deep_reverse(items){
    return is_null(items)
           ? null
           : is_pair(items)
           ? append(deep_reverse(tail(items)), 
                    pair(deep_reverse(head(items)), 
                         null))
           : items;
}

head(deep_reverse(x));

// expected: [ 4, [ 3, null ] ]
