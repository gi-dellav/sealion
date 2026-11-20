import { pair, stream, stream_ref } from './sicp.js';
const twos = pair(2, () => twos);
const sixes = scale_stream(twos, 3);
stream_ref(sixes, 50);
