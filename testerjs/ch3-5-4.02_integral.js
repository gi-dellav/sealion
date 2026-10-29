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
function scale_stream(stream, factor) {
    return stream_map(x => x * factor,
                      stream);
}
function integral(delayed_integrand, initial_value, dt) {
    const integ =
        pair(initial_value,
             () => {
                 const integrand = delayed_integrand();
                 return add_streams(scale_stream(integrand, dt), 
                                    integ);
             });
    return integ;
}

function numbers_starting_from(t, dt) {
    return pair(t,
                () => numbers_starting_from(t + dt, dt)
               );
}
const dt = 0.01;	
const linear = numbers_starting_from(0, dt);	       
const linear_integral = integral(() => linear, 0, dt);
// computing integral from 0 to 3 of f(x) = x
// (the integral is g(x) = 0.5 x^2, and therefore
// the result is near 0.5 * 3^2 = 4.5)
stream_ref(linear_integral, math_round(3 / dt));

// expected: 4.484999999999992
