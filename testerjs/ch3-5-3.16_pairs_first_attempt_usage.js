function stream_append(s1, s2) {
    return is_null(s1)
           ? s2
           : pair(head(s1), 
                  () => stream_append(stream_tail(s1), s2));
}
function pairs(s, t) {
    return pair(list(head(s), head(t)), 
                () => stream_append(
                         stream_map(x => list(head(s), x),
                                    stream_tail(t)),
                         pairs(stream_tail(s), stream_tail(t)))
               );
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
function integers_starting_from(n) {
    return pair(n, () => integers_starting_from(n + 1));
}
const integers = integers_starting_from(1);
pairs(integers, integers);

stream_ref(pairs(integers, integers), 8);

// expected: [ 1, [ 9, null ] ]
