function make_point(x, y){
    return pair(x, y);
}
function x_point(x){
    return head(x);
}
function y_point(x){
    return tail(x);
}

function make_rect(bottom_left, top_right){
    return pair(bottom_left, top_right);
}

function top_right(rect){
    return tail(rect);
}

function bottom_right(rect){
    return make_point(x_point(tail(rect)),
                      y_point(head(rect)));
}

function top_left(rect){
    return make_point(x_point(head(rect)),
                      y_point(tail(rect)));
}
  
function bottom_left(rect){
    return head(rect);
}
  
function abs(x){
    return x < 0 ? - x : x;
}

function width_rect(rect){
    return abs(x_point(bottom_left(rect)) - 
               x_point(bottom_right(rect)));
}

function height_rect(rect){
    return abs (y_point(bottom_left(rect)) - 
                y_point(top_left(rect)));
}

function area_rect(rect){
  return width_rect(rect) * height_rect(rect);
}

function perimeter_rect(rect){
  return 2 * (width_rect(rect) + height_rect(rect));
}

const v = make_rect(make_point(0, 1), make_point(2, 3));

perimeter_rect(v);

// expected: 8
