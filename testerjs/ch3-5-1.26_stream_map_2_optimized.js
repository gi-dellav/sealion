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
function stream_map_2(f, s1, s2) {
    return is_null(s1) || is_null(s2)
        ? null
        : pair(f(head(s1), head(s2)),
                () => stream_map_2(f, stream_tail(s1), stream_tail(s2)));
}

function stream_map_2_optimized(f, s1, s2) {
    return is_null(s1) || is_null(s2)
        ? null
        : pair(f(head(s1), head(s2)),
                memo(() => stream_map_2_optimized(f, stream_tail(s1), stream_tail(s2))));
}

const ints1 = integers_from(1);
const ints0 = integers_from(0);
const adds = (x, y) => x + y;

display(eval_stream(stream_map_2(adds, ints1, ints0), 5));
display(eval_stream(stream_map_2_optimized(adds, ints1, ints0), 5));
