const ints1 = integers_from(1);
const ints0 = integers_from(0);
const adds = (x, y) => x + y;

display(eval_stream(stream_map_2(adds, ints1, ints0), 5));
display(eval_stream(stream_map_2_optimized(adds, ints1, ints0), 5));
