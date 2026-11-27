import { append, pair, stream, stream_append, stream_ref } from './sicp.js';
const ones = pair(1, () => ones);
const twos = pair(2, () => twos);
const appended = stream_append(ones, twos);
stream_ref(appended, 100);
