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
function make_segment(v_start, v_end) {
    return pair(v_start, v_end);
}
function start_segment(v) {
    return head(v);
}
function end_segment(v) {
    return tail(v);
}
function make_frame(origin, edge1, edge2) {
    return list(origin, edge1, edge2);
}
function origin_frame(frame) {
    return list_ref(frame, 0);
}
function edge1_frame(frame) {
    return list_ref(frame, 1);
}
function edge2_frame(frame) {
    return list_ref(frame, 2);
}
function frame_coord_map(frame) {
    return v => add_vect(origin_frame(frame), 
                         add_vect(scale_vect(xcor_vect(v), 
                                             edge1_frame(frame)), 
                                  scale_vect(ycor_vect(v), 
                                             edge2_frame(frame))));
}
// "drawing a line" here simulated
// by printing the coordinates of
// the start and end of the line
function draw_line(v_start, v_end) {
    display("line starting at");
    display(v_start);
    display("line ending at");
    display(v_end);
}
function segments_to_painter(segment_list) {
    return frame => 
             for_each(segment =>
                        draw_line(
                            frame_coord_map(frame)
                                (start_segment(segment)), 
                            frame_coord_map(frame)
                                (end_segment(segment))), 
                      segment_list);
}
const unit_origin = make_vect(0, 0);
const unit_edge_1 = make_vect(1, 0);
const unit_edge_2 = make_vect(0, 1);
const unit_frame = make_frame(unit_origin, 
                              unit_edge_1,
                              unit_edge_2);
const diamond_start_1 = make_vect(0.5, 0);
const diamond_end_1 = make_vect(1, 0.5);
const diamond_segment_1 = make_segment(diamond_start_1, 
                                       diamond_end_1);
const diamond_start_2 = make_vect(1, 0.5);
const diamond_end_2 = make_vect(0.5, 1);
const diamond_segment_2 = make_segment(diamond_start_2, 
                                       diamond_end_2);
const diamond_start_3 = make_vect(0.5, 1);
const diamond_end_3 = make_vect(0, 0.5);
const diamond_segment_3 = make_segment(diamond_start_3, 
                                       diamond_end_3);
const diamond_start_4 = make_vect(0, 0.5);
const diamond_end_4 = make_vect(0.5, 0);
const diamond_segment_4 = make_segment(diamond_start_4, 
                                       diamond_end_4);
const diamond_painter = segments_to_painter(
                              list(diamond_segment_1, 
                                   diamond_segment_2,
                                   diamond_segment_3,
                                   diamond_segment_4));

diamond_painter(unit_frame);
