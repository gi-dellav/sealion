function average(x, y) {
    return (x + y) / 2;
}
function sqrt_improve(guess, x) {
    return average(guess, x / guess);
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
function sqrt_stream(x) {
   return pair(1, () => stream_map(guess => sqrt_improve(guess, x),
                                   sqrt_stream(x)));
}

stream_ref(sqrt_stream(2), 5);
