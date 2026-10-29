// chapter=2 
function stream_tail(stream) {
    return tail(stream)();
}
const max_display = 9;
function display_stream(s) {
    function display_stream_iter(st, n) {
        if (is_null(st)) {
        } else if (n === 0) {
            display('', "...");
        } else {
            display(head(st));
            display_stream_iter(stream_tail(st), n - 1);
        }
    }
    display_stream_iter(s, max_display);
}
function stream_filter(pred, stream) {
    return is_null(stream)
           ? null
           : pred(head(stream))
           ? pair(head(stream),
                  () => stream_filter(pred, stream_tail(stream)))
           : stream_filter(pred, stream_tail(stream));
}

const my_stream = pair(5, () => pair(6, () => pair(7, () => null)));
const my_filtered_stream =
    stream_filter(x => x % 2 === 0, my_stream);
head(my_filtered_stream);

// expected: 6
