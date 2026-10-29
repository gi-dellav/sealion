function integrate_series(s) {
    function helper(ss, iter) {
        return pair(head(ss) / iter, () => helper(stream_tail(ss), iter + 1));
    }
    return helper(s, 1);
}

const cos_series = pair(1, () => integrate_series(
        pair(0, 
            () => stream_map(
                    (x) => (-x), 
                    integrate_series(cos_series)))));
                    
const sin_series = pair(0, () => integrate_series(
        pair(1,
            () => integrate_series(
                stream_map(x => -x, sin_series)
                ))
        ));
