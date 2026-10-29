function enclosing_environment(env) { return tail(env); }

function first_frame(env) { return head(env); }

const the_empty_environment = null;
function make_frame(symbols, values) { return pair(symbols, values); }

function frame_symbols(frame) { return head(frame); }

function frame_values(frame) { return tail(frame); }
tail(head(extend_environment(list("x", "y", "z"),
                   list(1, 2, 3),
                   the_empty_environment)));
