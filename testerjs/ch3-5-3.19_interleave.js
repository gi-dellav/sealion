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
function interleave(s1, s2) {
    return is_null(s1)
           ? s2
           : pair(head(s1), 
                  () => interleave(s2, stream_tail(s1)));
}

const ones = pair(1, () => ones);
const twos = pair(2, () => twos);
const interleaved = interleave(ones, twos);
stream_ref(interleaved, 7);

// expected: 2
