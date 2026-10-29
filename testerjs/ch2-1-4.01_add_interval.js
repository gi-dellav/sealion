function make_interval(x, y) { 
    return pair(x, y); 
}
function lower_bound(i) { 
    return head(i);
}
function upper_bound(i) { 
    return tail(i);
}
// printing the interval in one line requires some string
// manipulation: stringify turns a number into a string 
// and the operator + can be applied to strings for
// string concatenation
function print_interval(i) {
    return "[ "  + stringify(lower_bound(i)) + 
           " , " + stringify(upper_bound(i)) + " ]";
}
function add_interval(x, y) {
    return make_interval(lower_bound(x) + lower_bound(y),
                         upper_bound(x) + upper_bound(y));
}

add_interval(make_interval(1, 2), 
             make_interval(3, 5));

// expected: [ 4, 7 ]
