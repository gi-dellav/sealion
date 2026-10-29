function scale_stream(stream, factor) {
    return stream_map(x => x * factor,
                      stream);
}

const twos = pair(2, () => twos);
const sixes = scale_stream(twos, 3);
stream_ref(sixes, 50);

// expected: 6
