// function equal to be written by student
function equal(xs, ys) {
    return is_pair(xs)
           ? (is_pair(ys) &&
              equal(head(xs), head(ys)) && 
              equal(tail(xs), tail(ys)))
           : is_null(xs)
           ? is_null(ys)
           : is_number(xs)
           ? (is_number(ys) && xs === ys)
           : is_boolean(xs)
           ? (is_boolean(ys) && ((xs && ys) || (!xs && !ys)))
           : is_string(xs)
           ? (is_string(ys) && xs === ys)
           : is_undefined(xs)
           ? is_undefined(ys)
           : // we know now that xs is a function
             (is_function(ys) && xs === ys);
}

equal(list("this", "is", "a", "list"), list("this", "is", "a", "list"));

// expected: true
