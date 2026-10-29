function make_point(x, y){
    return pair(x, y);
}

function make_rect(bottom_left, width, height){
    return pair(bottom_left, pair(width, height));
}

function height_rect(rect){
    return tail(tail(rect));
}

function width_rect(rect){
  return head(tail(rect));
}

function area_rect(rect){
  return width_rect(rect) * height_rect(rect);
}

function perimeter_rect(rect){
  return 2 * (width_rect(rect) + height_rect(rect));
}

const v = make_rect(make_point(1, 2), 2, 2);

perimeter_rect(v);
