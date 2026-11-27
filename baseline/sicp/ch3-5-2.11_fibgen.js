import { pair, stream, stream_ref } from './sicp.js';
function fibgen(a, b) {
    return pair(a, () => fibgen(b, a + b));
}

const fibs = fibgen(0, 1);

stream_ref(fibs, 50);

// expected: 12586269025
