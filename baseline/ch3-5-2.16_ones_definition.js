import { pair, stream, stream_ref } from './sicp.js';
const ones = pair(1, () => ones);

stream_ref(ones, 50);

// expected: 1
