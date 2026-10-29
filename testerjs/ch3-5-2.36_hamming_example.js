function integers_starting_from(n) {
    return pair(n, () => integers_starting_from(n + 1));
}
function scale_stream(stream, factor) {
    return stream_map(x => x * factor,
                      stream);
}
eval_stream(S, 50);
