function distinct(items) {	
    return is_null(items)
           ? true
           : is_null(tail(items))
           ? true
           : is_null(member(head(items), tail(items)))
           ? distinct(tail(items))
           : false;
}

distinct(list(1, 2, 4, 4, 5));

// expected: false
