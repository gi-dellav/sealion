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
function stream_map_optimized(f, s) {
    return is_null(s)
           ? null
           : pair(f(head(s)),
                  memo(() =>
                         stream_map_optimized(f, stream_tail(s))));
}

const my_stream = pair(4, () => pair(5, () => null));

const my_stream_2 =
    stream_map(x => x, my_stream);

stream_ref(my_stream_2, 1);
stream_ref(my_stream_2, 1);
// the number 5 is shown twice
// because the same delayed
// object is forced twice

let calls = 0;
const my_stream_3 =
    stream_map_optimized(x => {calls = calls + 1;}, my_stream);

stream_ref(my_stream_3, 1);
stream_ref(my_stream_3, 1);
calls;

// expected: 2
