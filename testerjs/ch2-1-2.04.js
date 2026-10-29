function print_point(p) {
    return display("(" + stringify(x_point(p)) + ", "
                       + stringify(y_point(p)) +        ")");
}
function x_point(x) {
    return head(x);
}
function y_point(x) {
    return tail(x);
}
function make_point(x, y) {
    return pair(x, y);
}
function make_segment(start_point, end_point) {
    return pair(start_point, end_point);
}
function start_segment(x) {
    return head(x);
}
function end_segment(x) {
    return tail(x);
}
function average(a, b) {
    return (a + b) / 2;
}
function mid_point_segment(x) {
    const a = start_segment(x);
    const b = end_segment(x);
    return make_point(average(x_point(a),
                              x_point(b)),
                      average(y_point(a),
                              y_point(b)));
}

const one_half = make_segment(make_point(0, 0), 
                              make_point(1, 1));
mid_point_segment(one_half);

// expected: [ 0.5, 0.5 ]
