function make_frame(symbols, values) { return pair(symbols, values); }

function frame_symbols(frame) { return head(frame); }

function frame_values(frame) { return tail(frame); }

const my_frame = make_frame(list("x", "y"), list(1, 2));
frame_symbols(my_frame);

// expected: [ 'x', [ 'y', null ] ]
