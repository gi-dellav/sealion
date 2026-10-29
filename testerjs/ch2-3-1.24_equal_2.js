// function equal to be written by student
function equal(xs, ys) {
    return is_pair(xs)
           ? (is_pair(ys) &&
              equal(head(xs), head(ys)) && 
              equal(tail(xs), tail(ys)))
           : xs === ys;
}

equal(list("this", "is", "a", "list"), list("this", "is", "a", "list"));

// expected: true
