function make_interval(x, y) {
    return pair(x, y);
}
function lower_bound(x) { 
    return head(x); 
}
function upper_bound(x) { 
    return tail(x); 
}
// printing the interval in one line requires some string
// manipulation: stringify turns a number into a string 
// and the operator + can be applied to strings for
// string concatenation
function print_interval(i) {
    return "[ "  + stringify(lower_bound(i)) + 
           " , " + stringify(upper_bound(i)) + " ]";
}
function p(n) {
    return n >= 0;
}
function n(n) {
    return ! p(n);
}
function the_trouble_maker(xl, xu, yl, yu) {
    const p1 = xl * yl;
    const p2 = xl * yu;
    const p3 = xu * yl;
    const p4 = xu * yu;
    return make_interval(math_min(p1, p2, p3, p4),
                  math_max(p1, p2, p3, p4));
}
function mul_interval(x, y) {  
    const xl = lower_bound(x);
    const xu = upper_bound(x);
    const yl = lower_bound(y);
    const yu = upper_bound(y);
    return p(xl) && p(xu) && p(yl) && p(yu)  
           ? make_interval(xl * yl, xu * yu)
           : p(xl) && p(xu) && n(yl) && p(yu)  
           ? make_interval(xu * yl, xu * yu)
           : p(xl) && p(xu) && n(yl) && n(yu)  
           ? make_interval(xu * yl, xl * yu)
           : n(xl) && p(xu) && p(yl) && p(yu)  
           ? make_interval(xl * yu, xu * yu)
           : n(xl) && p(xu) && n(yl) && n(yu)  
           ? make_interval(xu * yl, xl * yl)
           : n(xl) && n(xu) && p(yl) && p(yu)  
           ? make_interval(xl * yu, xu * yl)
           : n(xl) && n(xu) && n(yl) && p(yu)  
           ? make_interval(xl * yu, xl * yl)
           : n(xl) && n(xu) && n(yl) && n(yu)  
           ? make_interval(xu * yu, xl * yl)
           : n(xl) && p(xu) && n(yl) && p(yu)  
           ? the_trouble_maker(xl, xu, yl, yu)
           : error("lower larger than upper");
}

print_interval(mul_interval(make_interval(1, 2), 
                            make_interval(3, 5)));

// expected: '[ 3 , 10 ]'
