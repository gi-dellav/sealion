function make_interval(x, y) { 
    return pair(x, y); 
}
function lower_bound(i) { 
    return head(i);
}
function upper_bound(i) { 
    return tail(i);
}
function make_center_width(c, w) {
    return make_interval(c - w, c + w);
}
function center(i) {
    return (lower_bound(i) + upper_bound(i)) / 2;
}
function width(i) {
    return (upper_bound(i) - lower_bound(i)) / 2;
}
function make_center_percent(center, percent) {
    const width = center * (percent / 100);
    return make_center_width(center, width);
}
function percent(i) {
    return (width(i) / center(i)) * 100;
}

const my_interval = make_center_percent(6.0, 10);
percent(my_interval);

// expected: 9.999999999999993
