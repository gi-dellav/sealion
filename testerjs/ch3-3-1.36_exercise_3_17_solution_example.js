const three_list = list("a", "b", "c");
const one = pair("d", "e");
const two = pair(one, one);
const four_list = pair(two, "f");
const seven_list = pair(two, two);
const cycle = list("g", "h", "i");
set_tail(tail(tail(cycle)), cycle);

// return 3; return 3; return 3;
display(count_pairs(three_list));
display(count_pairs(four_list));
display(count_pairs(seven_list));

// return 3
display(count_pairs(cycle));
