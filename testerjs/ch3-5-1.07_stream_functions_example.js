function stream_tail(stream) {
    return tail(stream)();
}
const my_stream = pair(4, () => pair(5, () => null));
const my_stream_2 = stream_map(x => x + 1, my_stream);
let acc = 0;
stream_for_each(x => {acc = acc + x;}, my_stream_2);
acc;
