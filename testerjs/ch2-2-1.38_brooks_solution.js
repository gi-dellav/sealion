function plus_curried(x) {	    
    return y => x + y;
}
// brooks to be written by the student
function brooks(f, items) {
    return is_null(items)
           ? f
           : brooks(f(head(items)), tail(items));
}

brooks(plus_curried, list(3, 4));

// expected: 7
