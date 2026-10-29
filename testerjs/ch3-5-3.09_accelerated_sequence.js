function make_tableau(transform, s) {
    return pair(s, () => make_tableau(transform, transform(s)));
}
function square(x) {
    return x * x;
}
function memo(fun) {	    
    let already_run = false;
    let result = undefined;
    return () => {
               if (!already_run) {
                   result = fun();
                   already_run = true;
                   return result;
               } else {
                   return result;
               }
           };
}
function euler_transform(s) {
    const s0 = stream_ref(s, 0);     // $S_{n-1}$
    const s1 = stream_ref(s, 1);     // $S_{n}$
    const s2 = stream_ref(s, 2);     // $S_{n+1}$
    return pair(s2 - square(s2 - s1) / (s0 + (-2) * s1 + s2),
                memo(() => euler_transform(stream_tail(s))));
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
function scale_stream(stream, factor) {
    return stream_map(x => x * factor,
                      stream);
}
function stream_map_2(f, s1, s2) {
    return is_null(s1) && is_null(s2)
           ? null
           : is_null(s1) || is_null(s2)
           ? error(null, "unexpected argument -- stream_map_2")
           : pair(f(head(s1), head(s2)),
                  memo(() => stream_map_2(f, stream_tail(s1),
                                          stream_tail(s2))));
}
function add_streams(s1, s2) {
    return stream_map_2((x1, x2) => x1 + x2, s1, s2);
}
function partial_sums(s) {
    return pair(head(s),
                () => add_streams(stream_tail(s),
                                  partial_sums(s)));
}
function pi_summands(n) {
   return pair(1 / n, () => stream_map(x => - x, pi_summands(n + 2)));
}
const pi_stream = scale_stream(partial_sums(pi_summands(1)), 4);
function accelerated_sequence(transform, s) {
    return stream_map(head, make_tableau(transform, s));
}

stream_ref(accelerated_sequence(euler_transform, pi_stream),
           6);
