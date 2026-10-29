function stream_append(s1, s2) {
    return is_null(s1)
           ? s2
           : pair(head(s1), 
                  () => stream_append(stream_tail(s1), s2));
}
function pairs(s, t) {
    return pair(list(head(s), head(t)), 
                () => stream_append(
                         stream_map(x => list(head(s), x),
                                    stream_tail(t)),
                         pairs(stream_tail(s), stream_tail(t)))
               );
}
