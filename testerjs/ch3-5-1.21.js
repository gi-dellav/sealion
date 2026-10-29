function stream_tail(stream) {
    return tail(stream)();
}
function stream_map(f, s) {
    return is_null(s)
           ? null
           : pair(f(head(s)),
                  () => stream_map(f, stream_tail(s)));
}

const my_stream = pair(4, () => pair(5, () => null));
let calls = 0;
const my_stream_2 =
    stream_map(x => {calls = calls + 1;}, my_stream);

stream_ref(my_stream_2, 1);
stream_ref(my_stream_2, 1);
calls;

// expected: 3
