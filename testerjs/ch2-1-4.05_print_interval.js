// printing the interval in one line requires some string
// manipulation: stringify turns a number into a string 
// and the operator + can be applied to strings for
// string concatenation
function print_interval(i) {
    return "[ "  + stringify(lower_bound(i)) + 
           " , " + stringify(upper_bound(i)) + " ]";
}
