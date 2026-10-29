const three_list = list("a", "b", "c");
const cycle = list("g", "h", "i");
set_tail(tail(tail(cycle)), cycle);

// displays false
display(contains_cycle(three_list));

// displays true
display(contains_cycle(cycle));
