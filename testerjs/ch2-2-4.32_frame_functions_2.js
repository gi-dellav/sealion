function make_vect(x, y) {
    return pair(x, y);
}
function xcor_vect(vector) {
    return head(vector);
}
function ycor_vect(vector) {
    return tail(vector);
}
function scale_vect(factor, vector) {
    return make_vect(factor * xcor_vect(vector), 
                     factor * ycor_vect(vector));
}
function add_vect(vector1, vector2) {
    return make_vect(xcor_vect(vector1)  
                     + xcor_vect(vector2), 
                     ycor_vect(vector1)  
                     + ycor_vect(vector2));
}
function sub_vect(vector1, vector2) {
    return make_vect(xcor_vect(vector1)  
                     - xcor_vect(vector2), 
                     ycor_vect(vector1)  
                     - ycor_vect(vector2));
}
function make_frame(origin, edge1, edge2) {
    return pair(origin, pair(edge1, edge2));
}
function origin_frame(frame) {
    return head(frame);
}
function edge1_frame(frame) {
    return head(tail(frame));
}
function edge2_frame(frame) {
    return tail(tail(frame));
}

const my_origin = make_vect(1, 2);
const my_edge_1 = make_vect(3, 4);
const my_edge_2 = make_vect(5, 6);
const my_frame = make_frame(my_origin, my_edge_1, my_edge_2);
edge2_frame(my_frame);

// expected: [ 5, 6 ]
