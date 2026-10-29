function plus_curried(x) {	    
    return y => x + y;
}
function brooks(f, items) {
    return is_null(items)
           ? f
           : brooks(f(head(items)), tail(items));
}
// brooks_curried to be written by the student
function brooks_curried(items) {
    return brooks(head(items), tail(items));
}

brooks_curried(list(plus_curried, 3, 4));

// expected: 7
