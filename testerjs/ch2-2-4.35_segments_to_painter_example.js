const my_origin = make_vect(1, 2);
const my_edge_1 = make_vect(3, 4);
const my_edge_2 = make_vect(5, 6);
const my_frame = make_frame(my_origin, my_edge_1, my_edge_2);

const my_start_1 = make_vect(0, 1);
const my_end_1 = make_vect(1, 1);
const my_segment_1 = make_segment(my_start_1, my_end_1);

const my_start_2 = make_vect(0, 2);
const my_end_2 = make_vect(2, 2);
const my_segment_2 = make_segment(my_start_2, my_end_2);

const my_painter = segments_to_painter(
                       list(my_segment_1, my_segment_2));

my_painter(my_frame);
