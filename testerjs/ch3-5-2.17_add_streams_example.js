const ones = pair(1, () => ones);
const twos = pair(2, () => twos);
const threes = add_streams(ones, twos);
stream_ref(threes, 50);
