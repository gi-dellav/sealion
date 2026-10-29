const my_stream = pair(4, () => pair(5, () => null));

const my_stream_2 =
    stream_map(x => x, my_stream);

stream_ref(my_stream_2, 1);
stream_ref(my_stream_2, 1);
// the number 5 is shown twice
// because the same delayed
// object is forced twice

let calls = 0;
const my_stream_3 =
    stream_map_optimized(x => {calls = calls + 1;}, my_stream);

stream_ref(my_stream_3, 1);
stream_ref(my_stream_3, 1);
calls;
