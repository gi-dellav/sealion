function stream_tail(stream) {
    return tail(stream)();
}
const my_stream = pair(4, () => pair(5, () => null));
let calls = 0;
const my_stream_2 =
    stream_map(x => {calls = calls + 1;}, my_stream);

stream_ref(my_stream_2, 1);
stream_ref(my_stream_2, 1);
calls;
