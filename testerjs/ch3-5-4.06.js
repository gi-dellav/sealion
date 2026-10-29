function integral(integrand, initial_value, dt) {
    return pair(initial_value,
                is_null(integrand) 
                ? null
                : integral(stream_tail(integrand),
                           dt * head(integrand) + initial_value,
                           dt));
}
