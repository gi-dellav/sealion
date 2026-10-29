const twos = pair(2, () => twos);
const sixes = scale_stream(twos, 3);
stream_ref(sixes, 50);
