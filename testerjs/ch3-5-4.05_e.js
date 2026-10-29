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
// note the use of the memoization optimization		 
function stream_combine(f, s1, s2) {
    return is_null(s1) && is_null(s2)
           ? null
           : is_null(s1) || is_null(s2)
           ? error(null, "unexpected argument -- stream_combine")
           : pair(f(head(s1), head(s2)), 
                  memo(() => stream_combine(f, stream_tail(s1),
                                            stream_tail(s2))));
}
function add_streams(s1, s2) {
    return stream_combine((x1, x2) => x1 + x2, s1, s2);
}
function scale_stream(stream, factor) {
    return stream_map(x => x * factor,
                      stream);
}
// note the use of the memoization optimization		 
function integral(delayed_integrand, initial_value, dt) {
    const integ =
        pair(initial_value,
             memo(() => {
                  const integrand = delayed_integrand();
                  return add_streams(scale_stream(integrand, dt), 
                                     integ);
                 }));
    return integ;
}
function solve(f, y0, dt) {
    const y = integral(() => dy, y0, dt);
    const dy = stream_map(f, y);
    return y;
}
stream_ref(solve(y => y, 1, 0.001), 1000);

// expected: 2.716923932235896
