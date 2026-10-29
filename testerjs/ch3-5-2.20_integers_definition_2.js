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
const ones = pair(1, () => ones);
const integers = pair(1, () => add_streams(ones, integers));

stream_ref(integers, 50);

// expected: 51
