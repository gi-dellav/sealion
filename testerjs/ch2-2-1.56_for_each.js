// for_each to be given by student
function for_each(fun, items) {
    if (is_null(items)){
        return undefined;
    } else {
        fun(head(items));
        for_each(fun, tail(items));
    }
}

for_each(x => x, 
         list(57, 321, 88));

// expected: undefined
